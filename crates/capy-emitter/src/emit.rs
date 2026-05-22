//! S5b.1 lowering from [`Source`] to [`Module`].
//!
//! Walk strategy:
//!
//! * The [`ModuleEmitter`] iterates top-level [`Stmt`]s. Only `Item::Fn`
//!   and `Item::Import` are emitted; other forms produce a typed
//!   [`EmitError`] and are skipped.
//! * Per function, a [`FunctionEmitter`] holds the growing byte stream,
//!   a name → local-index map, a label table and a pending-jump list.
//!   Labels are assigned numeric ids; jump immediates are written as
//!   zero placeholders and patched after the function body is fully
//!   lowered.
//! * Constants are interned in a single module-wide [`ConstPoolBuilder`]
//!   so identical literal values share an index.
//!
//! Stack discipline: every `Expr` lowering pushes exactly one value;
//! every statement is stack-neutral. `Block` always produces one value
//! (its tail expression, or `LoadNone` when no tail is present).

#![forbid(unsafe_code)]

use std::collections::HashMap;

use capy_ast::{BinOp, Expr, Ident, Item, MatchArm, Pattern, Source, Span, Stmt, UnOp};
use capy_bytecode::{
    ConstPool, Constant, DebugEntry, DebugInfo, Function, FunctionTable, Import, ImportTable,
    Instruction, Module, Opcode, Section, SectionTag,
};

use crate::error::{EmitError, EmitErrorKind};

/// Result of [`emit`]: the produced module plus any per-item failures.
#[derive(Debug, Clone)]
pub struct EmitOutput {
    pub module: Module,
    pub errors: Vec<EmitError>,
}

/// Lowers `source` into a complete bytecode [`Module`].
///
/// Always returns: failed items are dropped and reported via
/// [`EmitOutput::errors`]; the resulting [`Module`] is internally
/// consistent and round-trips through [`Module::parse`].
#[must_use]
pub fn emit(source: &Source) -> EmitOutput {
    let mut me = ModuleEmitter::new();
    me.emit_source(source);
    me.finish()
}

struct ModuleEmitter {
    consts: ConstPoolBuilder,
    functions: Vec<Function>,
    imports: Vec<Import>,
    /// Source-level callable name → `imports[idx]`. Populated in pass 1
    /// from `Item::Import` declarations so that pass-2 body lowering can
    /// resolve a call to an imported `module::symbol` and emit a
    /// [`Instruction::HostCall`] instead of [`Instruction::Call`]. The
    /// key is the declaration's `as` alias when present, else the last
    /// segment of the import path.
    import_index: HashMap<String, u32>,
    errors: Vec<EmitError>,
    /// Stable function-name → function-table-index map. Populated in
    /// the first pass over the source so that subsequent body lowering
    /// can resolve `Call` targets regardless of forward / backward
    /// references inside the same module.
    fn_index: HashMap<String, u32>,
    /// Indices that have already been overwritten by a real emission
    /// during pass 2. Used to keep the *first* declaration canonical
    /// when the source contains duplicate `fn` items with the same
    /// name (the duplicate has already been reported by pass 1).
    emitted: std::collections::HashSet<u32>,
    /// Per-function debug entries collected by [`FunctionEmitter::finalize`].
    /// Indexed by `fn_idx`. The v0 `Debug` section has a single flat
    /// `bytecode_offset` field without a function discriminator, so the
    /// module-level encoder currently materialises only function 0's
    /// entries (typically `main`). A v1 debug section will add a
    /// function-index field; until then the rest are preserved here
    /// for inspection by future tooling.
    debug_per_fn: Vec<Vec<DebugEntry>>,
}

impl ModuleEmitter {
    fn new() -> Self {
        Self {
            consts: ConstPoolBuilder::default(),
            functions: Vec::new(),
            imports: Vec::new(),
            import_index: HashMap::new(),
            errors: Vec::new(),
            fn_index: HashMap::new(),
            emitted: std::collections::HashSet::new(),
            debug_per_fn: Vec::new(),
        }
    }

    fn emit_source(&mut self, source: &Source) {
        // Pass 1: pre-allocate stable indices for every `fn` and
        // `import` item so calls (both intra-module and cross-host) can
        // be resolved regardless of declaration order. Duplicate names
        // are reported and the second occurrence is skipped; the first
        // declaration keeps the index slot.
        for stmt in &source.stmts {
            match stmt {
                Stmt::Item(Item::Fn(f)) => {
                    if self.fn_index.contains_key(&f.name.name) {
                        self.errors.push(EmitError::new(
                            EmitErrorKind::DuplicateFunction {
                                name: f.name.name.clone(),
                            },
                            f.span,
                        ));
                        continue;
                    }
                    let idx = self.functions.len() as u32;
                    self.fn_index.insert(f.name.name.clone(), idx);
                    // Reserve the slot with a benign placeholder (returns
                    // `None`) so that an emission failure later does not
                    // shift any other function's index.
                    self.functions.push(Function {
                        name: f.name.name.clone(),
                        locals_count: 0,
                        code: vec![Opcode::LoadNone.as_byte(), Opcode::Return.as_byte()],
                    });
                    self.debug_per_fn.push(Vec::new());
                }
                Stmt::Item(Item::Import(i)) => self.collect_import(i),
                _ => {}
            }
        }

        // Pass 2: lower bodies in source order. Imports were already
        // materialised during pass 1.
        for stmt in &source.stmts {
            match stmt {
                Stmt::Item(item) => self.emit_item(item),
                Stmt::Let { span, .. } | Stmt::Expr { span, .. } => {
                    self.errors.push(EmitError::new(
                        EmitErrorKind::TopLevelMustBeItem,
                        *span,
                    ));
                }
            }
        }
    }

    fn emit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) => self.try_emit_fn(f),
            // Imports were materialised in pass 1; pass 2 is a no-op.
            Item::Import(_) => {}
            Item::Const(c) => self.errors.push(EmitError::new(
                EmitErrorKind::UnsupportedItem { what: "const" },
                c.span,
            )),
            Item::Struct(s) => self.errors.push(EmitError::new(
                EmitErrorKind::UnsupportedItem { what: "struct" },
                s.span,
            )),
            Item::TypeAlias(t) => self.errors.push(EmitError::new(
                EmitErrorKind::UnsupportedItem { what: "type alias" },
                t.span,
            )),
            Item::Enum(e) => self.errors.push(EmitError::new(
                EmitErrorKind::UnsupportedItem { what: "enum" },
                e.span,
            )),
        }
    }

    fn try_emit_fn(&mut self, f: &capy_ast::FnItem) {
        let fn_idx = match self.fn_index.get(&f.name.name).copied() {
            Some(i) => i,
            None => {
                // Pass 1 should always insert; defensive bail-out.
                return;
            }
        };
        if self.emitted.contains(&fn_idx) {
            // Duplicate declaration: pass 1 already reported the
            // collision and reserved the slot for the first
            // occurrence. Subsequent occurrences are dropped so the
            // first declaration's body remains canonical.
            return;
        }
        self.emitted.insert(fn_idx);
        let mut fe =
            FunctionEmitter::new(&mut self.consts, &self.fn_index, &self.import_index);
        // Register parameters as the first locals, in declaration order.
        // `locals[0]` corresponds to the first parameter, matching the
        // `Call` ABI in `docs/bytecode-v0.md`.
        for p in &f.params {
            if let Err(err) = fe.allocate_local(&p.name.name, p.span) {
                self.errors.push(err);
                return;
            }
        }
        if let Err(err) = fe.emit_expr(&f.body) {
            self.errors.push(err);
            return;
        }
        fe.emit_op(Opcode::Return);
        let locals_count = fe.locals_count;
        let (code, debug) = match fe.finalize() {
            Ok(pair) => pair,
            Err(err) => {
                self.errors.push(err);
                return;
            }
        };
        // Pass 1 reserved this slot, so a direct index assignment keeps
        // every other function's index stable across emission failures.
        self.functions[fn_idx as usize] = Function {
            name: f.name.name.clone(),
            locals_count,
            code,
        };
        self.debug_per_fn[fn_idx as usize] = debug;
    }

    /// Pass-1 helper: materialise an `Item::Import` into the bytecode
    /// `Imports` section AND register its source-level callable name in
    /// [`Self::import_index`] so that pass-2 body lowering can resolve
    /// `Expr::Call { callee: Ident(name), .. }` into a
    /// [`Instruction::HostCall`] addressed by `import_idx`.
    ///
    /// Mapping:
    ///
    /// - `import a::b::c;`        → `(module="a::b", symbol="c")`, name=`c`
    /// - `import a;`              → `(module="",     symbol="a")`, name=`a`
    /// - `import a::b::c as alias;` → `(module="a::b", symbol="c")`, name=`alias`
    ///
    /// Note: the `as` alias renames the **source-level callable name**
    /// only; the `(module, symbol)` pair the host adapter binds against
    /// continues to reflect the underlying import path so the wire
    /// surface stays decoupled from local renaming.
    fn collect_import(&mut self, i: &capy_ast::ImportItem) {
        let (module, symbol, default_name) = if i.path.len() == 1 {
            (String::new(), i.path[0].name.clone(), i.path[0].name.clone())
        } else {
            let last_idx = i.path.len() - 1;
            let module_parts: Vec<&str> =
                i.path[..last_idx].iter().map(|s| s.name.as_str()).collect();
            let module = module_parts.join("::");
            let symbol = i.path[last_idx].name.clone();
            let default_name = symbol.clone();
            (module, symbol, default_name)
        };
        let name = i
            .alias
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or(default_name);

        if self.import_index.contains_key(&name) {
            self.errors.push(EmitError::new(
                EmitErrorKind::DuplicateImport { name },
                i.span,
            ));
            return;
        }
        let idx = self.imports.len() as u32;
        self.import_index.insert(name, idx);
        self.imports.push(Import { module, symbol });
    }

    fn finish(self) -> EmitOutput {
        let consts_payload = self.consts.build().encode();
        let functions_payload = FunctionTable {
            entries: self.functions,
        }
        .encode();
        let imports_payload = ImportTable {
            entries: self.imports,
        }
        .encode();
        // v0 `Debug` section is per-function-local but lacks a
        // function discriminator. Until v1 adds one, the module
        // materialises only function 0's debug entries (typically
        // `main`); other functions' debug streams are dropped here
        // but preserved in `debug_per_fn` for in-process inspection.
        // A non-empty debug payload only appears when at least one
        // entry was actually recorded — emitters running on an empty
        // source produce no `Debug` section at all so byte-identical
        // round-trips with pre-debug emitter output stay possible.
        let debug_payload = self
            .debug_per_fn
            .first()
            .filter(|entries| !entries.is_empty())
            .map(|entries| {
                DebugInfo {
                    entries: entries.clone(),
                }
                .encode()
            });
        let mut sections = vec![
            Section::new(SectionTag::Consts, consts_payload),
            Section::new(SectionTag::Functions, functions_payload),
            Section::new(SectionTag::Imports, imports_payload),
        ];
        if let Some(payload) = debug_payload {
            sections.push(Section::new(SectionTag::Debug, payload));
        }
        let module = Module::new(0, sections);
        EmitOutput {
            module,
            errors: self.errors,
        }
    }
}

#[derive(Debug, Default)]
struct ConstPoolBuilder {
    entries: Vec<Constant>,
    int_idx: HashMap<i64, u32>,
    float_bits_idx: HashMap<u64, u32>,
    str_idx: HashMap<String, u32>,
}

impl ConstPoolBuilder {
    fn intern_int(&mut self, v: i64) -> u32 {
        if let Some(&i) = self.int_idx.get(&v) {
            return i;
        }
        let i = self.entries.len() as u32;
        self.entries.push(Constant::Int(v));
        self.int_idx.insert(v, i);
        i
    }

    fn intern_float(&mut self, v: f64) -> u32 {
        let bits = v.to_bits();
        if let Some(&i) = self.float_bits_idx.get(&bits) {
            return i;
        }
        let i = self.entries.len() as u32;
        self.entries.push(Constant::Float(v));
        self.float_bits_idx.insert(bits, i);
        i
    }

    fn intern_str(&mut self, s: String) -> u32 {
        if let Some(&i) = self.str_idx.get(&s) {
            return i;
        }
        let i = self.entries.len() as u32;
        self.entries.push(Constant::Str(s.clone()));
        self.str_idx.insert(s, i);
        i
    }

    fn build(self) -> ConstPool {
        ConstPool {
            entries: self.entries,
        }
    }
}

/// Resolved target of an `Expr::Call`. The emitter picks one of these
/// variants per call site based on the callee name's resolution in
/// the module-level `fn_index` / `import_index` maps; the surface
/// syntax `name(args)` is identical either way.
#[derive(Debug, Clone, Copy)]
enum CallTarget {
    /// In-module top-level `fn` at the given function-table index.
    /// Lowers to [`Instruction::Call`].
    Local(u32),
    /// Imported `module::symbol` at the given import-table index.
    /// Lowers to [`Instruction::HostCall`] for dispatch through the
    /// VM's `HostAdapter` at run time.
    Host(u32),
}

/// Active loop frame consulted by `break` and `continue`.
///
/// The structural difference between `while` and `loop` (whether the
/// break value is discarded at the join point) is encoded directly in
/// the surrounding emitter code, not in the frame, so a single shape
/// covers both forms.
#[derive(Debug, Clone, Copy)]
struct LoopCtx {
    continue_label: u32,
    break_label: u32,
}

struct FunctionEmitter<'a> {
    consts: &'a mut ConstPoolBuilder,
    /// Read-only view of the module-level function-name → index map
    /// built by [`ModuleEmitter::emit_source`]'s first pass. Used to
    /// resolve `Expr::Call { callee: Ident(name), .. }` targets that
    /// refer to in-module `fn` items.
    fn_index: &'a HashMap<String, u32>,
    /// Read-only view of the module-level callable-name → import-index
    /// map built by [`ModuleEmitter::collect_import`]. Used by
    /// `emit_call` to lower a call whose callee resolves to an imported
    /// symbol into [`Instruction::HostCall`] addressed by `import_idx`.
    import_index: &'a HashMap<String, u32>,
    code: Vec<u8>,
    locals: HashMap<String, u32>,
    locals_count: u32,
    next_label: u32,
    labels: HashMap<u32, u32>,
    pending_jumps: Vec<(usize, u32, Span)>,
    loop_stack: Vec<LoopCtx>,
    /// Active source-span stack maintained by [`Self::emit_expr`]: the
    /// innermost `Expr` currently being lowered sits at the top. Every
    /// instruction emission records a `DebugEntry` against the top of
    /// this stack so the bytecode-offset → source-span mapping reflects
    /// the most specific syntactic node that produced the opcode.
    debug_span_stack: Vec<Span>,
    /// Accumulated debug entries for the function, in increasing
    /// bytecode-offset order. Encoded into the module's optional
    /// `Debug` section by [`ModuleEmitter::finish`].
    debug_entries: Vec<DebugEntry>,
}

impl<'a> FunctionEmitter<'a> {
    fn new(
        consts: &'a mut ConstPoolBuilder,
        fn_index: &'a HashMap<String, u32>,
        import_index: &'a HashMap<String, u32>,
    ) -> Self {
        Self {
            consts,
            fn_index,
            import_index,
            code: Vec::new(),
            locals: HashMap::new(),
            locals_count: 0,
            next_label: 0,
            labels: HashMap::new(),
            pending_jumps: Vec::new(),
            loop_stack: Vec::new(),
            debug_span_stack: Vec::new(),
            debug_entries: Vec::new(),
        }
    }

    /// Records a `DebugEntry` for the instruction about to be written
    /// at the current `self.code.len()` offset against the innermost
    /// active source span (top of [`Self::debug_span_stack`]).
    ///
    /// Consecutive calls at the same `bytecode_offset` overwrite the
    /// last entry so the most-recent active span wins. This matters
    /// when a parent `Expr` records its span before recursing into a
    /// child whose first opcode lands at the same offset — the
    /// child's narrower span is preferred so the debug map points at
    /// the smallest syntactic source for any given pc.
    fn record_debug(&mut self) {
        let span = match self.debug_span_stack.last() {
            Some(&s) => s,
            None => return,
        };
        let offset = self.code.len() as u32;
        if let Some(last) = self.debug_entries.last_mut() {
            if last.bytecode_offset == offset {
                // Same offset → prefer the narrower (innermost) span.
                last.source_start = span.start as u32;
                last.source_end = span.end as u32;
                return;
            }
        }
        self.debug_entries.push(DebugEntry {
            bytecode_offset: offset,
            source_start: span.start as u32,
            source_end: span.end as u32,
        });
    }

    fn emit_op(&mut self, op: Opcode) {
        self.record_debug();
        self.code.push(op.as_byte());
    }

    fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    fn new_label(&mut self) -> u32 {
        let id = self.next_label;
        self.next_label += 1;
        id
    }

    fn mark_label(&mut self, id: u32) {
        self.labels.insert(id, self.code.len() as u32);
    }

    fn emit_jump(&mut self, op: Opcode, target: u32, span: Span) {
        self.record_debug();
        self.code.push(op.as_byte());
        let site = self.code.len();
        self.code.extend_from_slice(&0i32.to_le_bytes());
        self.pending_jumps.push((site, target, span));
    }

    /// Finalises the function's byte stream and emits the accumulated
    /// debug entries alongside it.
    ///
    /// Pending forward jumps are resolved against [`Self::labels`] in
    /// place; the resulting `code` is shape-compatible with the v0
    /// wire format. `debug` carries the [`DebugEntry`] list collected
    /// during emission, in increasing `bytecode_offset` order — the
    /// caller decides whether to materialise it into the module's
    /// optional `Debug` section.
    fn finalize(self) -> Result<(Vec<u8>, Vec<DebugEntry>), EmitError> {
        let mut code = self.code;
        for (site, label, span) in self.pending_jumps {
            let target = match self.labels.get(&label) {
                Some(&t) => t,
                None => {
                    // Internal invariant: every label that was jumped to
                    // must have been marked. If this triggers, it points
                    // to a bug in the emitter rather than malformed
                    // input — emit a synthetic Unsupported error so the
                    // failure surfaces deterministically.
                    return Err(EmitError::new(
                        EmitErrorKind::UnsupportedFeature {
                            what: "unresolved internal label",
                        },
                        span,
                    ));
                }
            };
            let after = (site + 4) as i32;
            let offset = (target as i32) - after;
            let bytes = offset.to_le_bytes();
            code[site..site + 4].copy_from_slice(&bytes);
        }
        Ok((code, self.debug_entries))
    }

    fn allocate_local(&mut self, name: &str, span: Span) -> Result<u32, EmitError> {
        if self.locals.contains_key(name) {
            return Err(EmitError::new(
                EmitErrorKind::DuplicateLocal {
                    name: name.to_string(),
                },
                span,
            ));
        }
        let idx = self.locals_count;
        self.locals.insert(name.to_string(), idx);
        self.locals_count += 1;
        Ok(idx)
    }

    /// Lowering entry-point for any expression.
    ///
    /// Maintains the per-`Expr` active-span stack used by
    /// [`Self::record_debug`] so the resulting `Debug` section maps
    /// every opcode back to the innermost syntactic node that emitted
    /// it. The actual lowering match is in [`Self::emit_expr_inner`];
    /// this wrapper only handles the push / pop discipline so an
    /// early-return (`?` propagation on a sub-expression error) cannot
    /// leak a stale span entry.
    fn emit_expr(&mut self, expr: &Expr) -> Result<(), EmitError> {
        self.debug_span_stack.push(expr.span());
        let result = self.emit_expr_inner(expr);
        self.debug_span_stack.pop();
        result
    }

    fn emit_expr_inner(&mut self, expr: &Expr) -> Result<(), EmitError> {
        match expr {
            Expr::Int { text, span } => {
                let v = parse_int_literal(text).ok_or_else(|| {
                    EmitError::new(
                        EmitErrorKind::IntegerParse {
                            text: text.clone(),
                        },
                        *span,
                    )
                })?;
                let idx = self.consts.intern_int(v);
                self.emit_op(Opcode::LoadConst);
                self.emit_u32(idx);
            }
            Expr::Float { text, span } => {
                let v = parse_float_literal(text).ok_or_else(|| {
                    EmitError::new(
                        EmitErrorKind::FloatParse {
                            text: text.clone(),
                        },
                        *span,
                    )
                })?;
                let idx = self.consts.intern_float(v);
                self.emit_op(Opcode::LoadConst);
                self.emit_u32(idx);
            }
            Expr::Str { text, span } => {
                let s = parse_str_literal(text).map_err(|reason| {
                    EmitError::new(EmitErrorKind::StringParse { reason }, *span)
                })?;
                let idx = self.consts.intern_str(s);
                self.emit_op(Opcode::LoadConst);
                self.emit_u32(idx);
            }
            Expr::Bool { value, .. } => {
                self.emit_op(if *value {
                    Opcode::LoadTrue
                } else {
                    Opcode::LoadFalse
                });
            }
            Expr::NoneLit { .. } => self.emit_op(Opcode::LoadNone),
            Expr::Ident(id) => self.emit_ident_load(id)?,
            Expr::Paren { inner, .. } => self.emit_expr(inner)?,
            Expr::Unary { op, operand, span } => {
                self.emit_expr(operand)?;
                match op {
                    UnOp::Neg => self.emit_op(Opcode::Neg),
                    UnOp::Not => self.emit_op(Opcode::Not),
                    UnOp::BitNot => {
                        return Err(EmitError::new(
                            EmitErrorKind::UnsupportedUnary { op: "BitNot" },
                            *span,
                        ));
                    }
                }
            }
            Expr::Binary { op, lhs, rhs, span } => self.emit_binary(*op, lhs, rhs, *span)?,
            Expr::Block { stmts, tail, .. } => self.emit_block(stmts, tail.as_deref())?,
            Expr::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => self.emit_if(cond, then_branch, else_branch.as_deref())?,
            Expr::Return { value, .. } => self.emit_return(value.as_deref())?,
            Expr::Path { span, .. } => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedExpr { what: "path" },
                    *span,
                ));
            }
            Expr::Call { callee, args, span } => self.emit_call(callee, args, *span)?,
            Expr::Index { span, .. } => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedExpr { what: "index" },
                    *span,
                ));
            }
            Expr::Field { span, .. } => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedExpr { what: "field" },
                    *span,
                ));
            }
            Expr::While { cond, body, span } => self.emit_while(cond, body, *span)?,
            Expr::Loop { body, span } => self.emit_loop(body, *span)?,
            Expr::Break { value, span } => self.emit_break(value.as_deref(), *span)?,
            Expr::Continue { span } => self.emit_continue(*span)?,
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.emit_match(scrutinee, arms, *span)?,
            Expr::Error { span } => {
                return Err(EmitError::new(EmitErrorKind::ParseErrorInExpr, *span));
            }
        }
        Ok(())
    }

    fn emit_ident_load(&mut self, id: &Ident) -> Result<(), EmitError> {
        let idx = self
            .locals
            .get(&id.name)
            .copied()
            .ok_or_else(|| {
                EmitError::new(
                    EmitErrorKind::UnknownLocal {
                        name: id.name.clone(),
                    },
                    id.span,
                )
            })?;
        self.emit_op(Opcode::LoadLocal);
        self.emit_u32(idx);
        Ok(())
    }

    fn emit_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<(), EmitError> {
        let opcode = match op {
            BinOp::Add => Opcode::Add,
            BinOp::Sub => Opcode::Sub,
            BinOp::Mul => Opcode::Mul,
            BinOp::Div => Opcode::Div,
            BinOp::Mod => Opcode::Mod,
            BinOp::Eq => Opcode::Eq,
            BinOp::Ne => Opcode::Ne,
            BinOp::Lt => Opcode::Lt,
            BinOp::Le => Opcode::Le,
            BinOp::Gt => Opcode::Gt,
            BinOp::Ge => Opcode::Ge,
            BinOp::And => return self.emit_and(lhs, rhs),
            BinOp::Or => return self.emit_or(lhs, rhs),
            BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Shl
            | BinOp::Shr => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedBinary { op: op.as_str() },
                    span,
                ));
            }
        };
        self.emit_expr(lhs)?;
        self.emit_expr(rhs)?;
        self.emit_op(opcode);
        Ok(())
    }

    fn emit_block(&mut self, stmts: &[Stmt], tail: Option<&Expr>) -> Result<(), EmitError> {
        for stmt in stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(t) = tail {
            self.emit_expr(t)?;
        } else {
            self.emit_op(Opcode::LoadNone);
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), EmitError> {
        match stmt {
            Stmt::Let { name, init, span, .. } => {
                if let Some(init) = init {
                    self.emit_expr(init)?;
                } else {
                    self.emit_op(Opcode::LoadNone);
                }
                let idx = self.allocate_local(&name.name, *span)?;
                self.emit_op(Opcode::StoreLocal);
                self.emit_u32(idx);
            }
            Stmt::Expr { expr, .. } => {
                // Every expression statement leaves no net value: the
                // expression is emitted then popped, regardless of
                // `has_semi`. The block tail handles the producing
                // value separately.
                self.emit_expr(expr)?;
                self.emit_op(Opcode::Pop);
            }
            Stmt::Item(item) => {
                return Err(EmitError::new(EmitErrorKind::NestedItem, item.span()));
            }
        }
        Ok(())
    }

    fn emit_if(
        &mut self,
        cond: &Expr,
        then_branch: &Expr,
        else_branch: Option<&Expr>,
        // span omitted: callers ensure span context via primary tokens
    ) -> Result<(), EmitError> {
        self.emit_expr(cond)?;
        let else_label = self.new_label();
        let end_label = self.new_label();
        self.emit_jump(Opcode::JumpIfFalse, else_label, then_branch.span());
        self.emit_expr(then_branch)?;
        self.emit_jump(Opcode::Jump, end_label, then_branch.span());
        self.mark_label(else_label);
        if let Some(eb) = else_branch {
            self.emit_expr(eb)?;
        } else {
            self.emit_op(Opcode::LoadNone);
        }
        self.mark_label(end_label);
        Ok(())
    }

    /// Short-circuit logical AND. Lowering pattern:
    ///
    /// ```text
    ///   emit lhs
    ///   jump_if_false short      ; pops lhs; if false, jump
    ///   emit rhs                 ; otherwise, result is rhs
    ///   jump end
    /// short:
    ///   load_false               ; lhs was false → result false
    /// end:
    /// ```
    ///
    /// Net stack effect: +1.
    fn emit_and(&mut self, lhs: &Expr, rhs: &Expr) -> Result<(), EmitError> {
        let short = self.new_label();
        let end = self.new_label();
        self.emit_expr(lhs)?;
        self.emit_jump(Opcode::JumpIfFalse, short, lhs.span());
        self.emit_expr(rhs)?;
        self.emit_jump(Opcode::Jump, end, rhs.span());
        self.mark_label(short);
        self.emit_op(Opcode::LoadFalse);
        self.mark_label(end);
        Ok(())
    }

    /// Short-circuit logical OR. Lowering pattern (using only
    /// `JumpIfFalse`):
    ///
    /// ```text
    ///   emit lhs
    ///   jump_if_false try_rhs    ; pops lhs; if false, evaluate rhs
    ///   load_true                ; lhs was true → result true
    ///   jump end
    /// try_rhs:
    ///   emit rhs                 ; result is rhs
    /// end:
    /// ```
    ///
    /// Net stack effect: +1.
    fn emit_or(&mut self, lhs: &Expr, rhs: &Expr) -> Result<(), EmitError> {
        let try_rhs = self.new_label();
        let end = self.new_label();
        self.emit_expr(lhs)?;
        self.emit_jump(Opcode::JumpIfFalse, try_rhs, lhs.span());
        self.emit_op(Opcode::LoadTrue);
        self.emit_jump(Opcode::Jump, end, lhs.span());
        self.mark_label(try_rhs);
        self.emit_expr(rhs)?;
        self.mark_label(end);
        Ok(())
    }

    /// `while <cond> <body>` lowering. The expression always evaluates
    /// to `None`; any value carried by an inner `break` is discarded at
    /// the join point.
    ///
    /// ```text
    /// loop_start:
    ///   emit cond
    ///   jump_if_false fallthrough
    ///   emit body                ; pushes 1 (block tail value)
    ///   pop                      ; drop body value
    ///   jump loop_start
    /// break_label:               ; reached by `break <v>` with v on stack
    ///   pop                      ; discard break value
    /// fallthrough:
    ///   load_none                ; while expression result
    /// ```
    ///
    /// `continue` targets `loop_start`. The slice assumes the typical
    /// usage `if ... { break; }` / `if ... { continue; }` at statement
    /// position, where the local operand stack is empty before the
    /// jump. Reaching `break` / `continue` from inside a partially
    /// evaluated expression is accepted by the emitter but may leave
    /// dead values on the stack at runtime — a known limitation of
    /// S5b.2 that will be tightened once a stack-balance verifier
    /// lands in a later slice.
    fn emit_while(&mut self, cond: &Expr, body: &Expr, span: Span) -> Result<(), EmitError> {
        let loop_start = self.new_label();
        let break_label = self.new_label();
        let fallthrough = self.new_label();

        self.mark_label(loop_start);
        self.emit_expr(cond)?;
        self.emit_jump(Opcode::JumpIfFalse, fallthrough, span);

        self.loop_stack.push(LoopCtx {
            continue_label: loop_start,
            break_label,
        });
        let body_result = self.emit_expr(body);
        self.loop_stack.pop();
        body_result?;

        self.emit_op(Opcode::Pop);
        self.emit_jump(Opcode::Jump, loop_start, span);

        self.mark_label(break_label);
        self.emit_op(Opcode::Pop);
        self.mark_label(fallthrough);
        self.emit_op(Opcode::LoadNone);
        Ok(())
    }

    /// `loop <body>` lowering. The expression value is the value
    /// carried by the `break` that terminates the loop (or `None` when
    /// `break` carries no value).
    ///
    /// ```text
    /// loop_start:
    ///   emit body                ; pushes 1
    ///   pop
    ///   jump loop_start
    /// break_label:               ; break leaves +1 on stack → loop result
    /// ```
    fn emit_loop(&mut self, body: &Expr, span: Span) -> Result<(), EmitError> {
        let loop_start = self.new_label();
        let break_label = self.new_label();

        self.mark_label(loop_start);

        self.loop_stack.push(LoopCtx {
            continue_label: loop_start,
            break_label,
        });
        let body_result = self.emit_expr(body);
        self.loop_stack.pop();
        body_result?;

        self.emit_op(Opcode::Pop);
        self.emit_jump(Opcode::Jump, loop_start, span);

        self.mark_label(break_label);
        Ok(())
    }

    /// `break [<value>]` lowering. Pushes the break payload (the value
    /// or `LoadNone`) then jumps to the innermost loop's `break_label`.
    /// The payload is consumed by the `while` join (which discards it)
    /// or kept as the `loop` expression's value.
    fn emit_break(&mut self, value: Option<&Expr>, span: Span) -> Result<(), EmitError> {
        let break_label = self
            .loop_stack
            .last()
            .map(|c| c.break_label)
            .ok_or_else(|| EmitError::new(EmitErrorKind::BreakOutsideLoop, span))?;
        if let Some(v) = value {
            self.emit_expr(v)?;
        } else {
            self.emit_op(Opcode::LoadNone);
        }
        self.emit_jump(Opcode::Jump, break_label, span);
        Ok(())
    }

    /// `continue` lowering. Jumps to the innermost loop's
    /// `continue_label` (i.e. the loop header, before the condition is
    /// re-evaluated for `while` or the body re-enters for `loop`).
    fn emit_continue(&mut self, span: Span) -> Result<(), EmitError> {
        let continue_label = self
            .loop_stack
            .last()
            .map(|c| c.continue_label)
            .ok_or_else(|| EmitError::new(EmitErrorKind::ContinueOutsideLoop, span))?;
        self.emit_jump(Opcode::Jump, continue_label, span);
        Ok(())
    }

    /// Lowering for `callee(arg0, arg1, ...)`.
    ///
    /// Direct calls to top-level `fn` items lower to `Call`; direct
    /// calls to imported `module::symbol` items lower to `HostCall`.
    /// Other callee shapes (path, field access, parenthesised
    /// expression, dynamic value) are rejected with
    /// `UnsupportedCallee`. Local `fn` items shadow imports of the same
    /// name (mirroring Rust's `use` precedence). Arguments are
    /// evaluated strictly left-to-right and pushed in source order; the
    /// resulting `(Host)Call` instruction pops them into the callee's
    /// `locals[0..argc]` (for `Call`) or hands them to the host adapter
    /// as a borrowed slice (for `HostCall`).
    fn emit_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Result<(), EmitError> {
        let name = match callee {
            Expr::Ident(id) => &id.name,
            Expr::Path { span, .. } => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedCallee { what: "path" },
                    *span,
                ));
            }
            other => {
                return Err(EmitError::new(
                    EmitErrorKind::UnsupportedCallee {
                        what: "non-identifier callee",
                    },
                    other.span(),
                ));
            }
        };
        let target = if let Some(&fn_idx) = self.fn_index.get(name) {
            CallTarget::Local(fn_idx)
        } else if let Some(&import_idx) = self.import_index.get(name) {
            CallTarget::Host(import_idx)
        } else {
            return Err(EmitError::new(
                EmitErrorKind::UnknownFunction { name: name.clone() },
                span,
            ));
        };
        if args.len() > u32::MAX as usize {
            return Err(EmitError::new(
                EmitErrorKind::TooManyArguments { count: args.len() },
                span,
            ));
        }
        for a in args {
            self.emit_expr(a)?;
        }
        let argc = args.len() as u32;
        // Encode via the typed `Instruction` codec so the wire layout
        // stays in lockstep with `capy-bytecode`.
        match target {
            CallTarget::Local(fn_idx) => {
                self.record_debug();
                Instruction::Call { fn_idx, argc }.encode_into(&mut self.code);
            }
            CallTarget::Host(import_idx) => {
                self.record_debug();
                Instruction::HostCall { import_idx, argc }.encode_into(&mut self.code);
            }
        }
        Ok(())
    }

    /// Allocates a fresh, unnamed local slot. Used for synthetic
    /// per-`match` scrutinee storage so emitting nested matches stays
    /// hygienic without invading the user-visible local namespace.
    fn alloc_unnamed_local(&mut self) -> u32 {
        let idx = self.locals_count;
        self.locals_count += 1;
        idx
    }

    /// Lowering for `match scrut { arm0, arm1, ... }` (S2.2b → emitter).
    ///
    /// Strategy (literal + wildcard + ident-binding first cut):
    ///
    /// ```text
    ///   <evaluate scrut>
    ///   store_local scrut_slot
    ///   ; arm 0
    ///   <test pattern_0 against LoadLocal(scrut_slot)>      ; pushes Bool
    ///   jump_if_false next_arm_label_0
    ///   <bind any pattern_0 identifiers>
    ///   <evaluate guard_0 (if any)>; jump_if_false next_arm_label_0
    ///   <evaluate body_0>
    ///   jump match_end
    ///   next_arm_label_0:
    ///   ; arm 1, 2, ...                                     same shape
    ///   load_none                                           ; fallback
    ///   match_end:
    /// ```
    ///
    /// Net stack effect: +1 (the chosen body's value, or `None` if no
    /// arm matched). Pattern bindings live in fresh locals allocated
    /// per arm; the `self.locals` map is saved before each arm and
    /// restored after, so a binding introduced by one arm cannot leak
    /// into a sibling arm or the surrounding scope.
    ///
    /// Unsupported pattern kinds (range, tuple-struct, struct, path,
    /// or-patterns, rest) produce a typed `UnsupportedFeature` error
    /// at the offending arm; emission of the other arms continues so
    /// partial diagnostics remain useful.
    fn emit_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<(), EmitError> {
        // Evaluate the scrutinee once and stash it.
        self.emit_expr(scrutinee)?;
        let scrut_slot = self.alloc_unnamed_local();
        self.emit_op(Opcode::StoreLocal);
        self.emit_u32(scrut_slot);

        let end_label = self.new_label();
        for arm in arms {
            let next_arm = self.new_label();
            // Save the locals map so arm-local bindings cannot leak.
            let saved_locals = self.locals.clone();

            // Pattern test: pushes Bool, falls through to body on
            // match, or branches to next_arm on no-match. Wildcard
            // and Ident patterns always match (no test emitted).
            self.emit_pattern_test(&arm.pattern, scrut_slot, next_arm)?;
            // Pattern bindings (only Ident for the first cut).
            self.emit_pattern_bindings(&arm.pattern, scrut_slot)?;
            // Optional guard.
            if let Some(guard) = &arm.guard {
                self.emit_expr(guard)?;
                self.emit_jump(Opcode::JumpIfFalse, next_arm, guard.span());
            }
            // Body.
            self.emit_expr(&arm.body)?;
            self.emit_jump(Opcode::Jump, end_label, arm.span);

            // Restore locals for the next arm.
            self.locals = saved_locals;
            self.mark_label(next_arm);
        }
        // Fall-through (no arm matched). The verifier sees the same
        // stack depth on every path because each arm exits via the
        // Jump above with exactly one value pushed; the fall-through
        // pushes one via `LoadNone`.
        let _ = span; // currently informational; reserved for future
                       // exhaustiveness diagnostics.
        self.emit_op(Opcode::LoadNone);
        self.mark_label(end_label);
        Ok(())
    }

    /// Emits the comparison that selects between "this arm matches"
    /// and "try the next arm".
    ///
    /// On entry the operand stack is at its pre-test depth. On exit
    /// (when the pattern actually emits a test) the stack is unchanged
    /// for the match-success path; the branch path consumes the
    /// pushed Bool via `JumpIfFalse`.
    fn emit_pattern_test(
        &mut self,
        pattern: &Pattern,
        scrut_slot: u32,
        next_arm: u32,
    ) -> Result<(), EmitError> {
        match pattern {
            // Always match; no test needed.
            Pattern::Wildcard { .. } | Pattern::Ident(_) => Ok(()),
            Pattern::Literal { value, .. } => {
                // Push scrut, push literal, compare with Eq, branch
                // on false. The Eq opcode tolerates Int/Float mixed
                // pairs per the v0 comparison contract.
                self.emit_op(Opcode::LoadLocal);
                self.emit_u32(scrut_slot);
                self.emit_expr(value)?;
                self.emit_op(Opcode::Eq);
                self.emit_jump(Opcode::JumpIfFalse, next_arm, value.span());
                Ok(())
            }
            Pattern::Range {
                lo,
                hi,
                inclusive,
                span,
            } => {
                // Lower `lo..hi` to two bounds checks:
                //   load_local scrut; load_const lo; ge
                //   jump_if_false next_arm
                //   load_local scrut; load_const hi; (le | lt)
                //   jump_if_false next_arm
                // Net stack effect: 0 (each JumpIfFalse pops its Bool).
                // Endpoints must be literal patterns; the parser
                // already restricts the grammar but we re-check here
                // so a future grammar relaxation cannot silently emit
                // wrong code.
                let lo_value = Self::pattern_as_literal_value(lo)?;
                let hi_value = Self::pattern_as_literal_value(hi)?;
                self.emit_op(Opcode::LoadLocal);
                self.emit_u32(scrut_slot);
                self.emit_expr(lo_value)?;
                self.emit_op(Opcode::Ge);
                self.emit_jump(Opcode::JumpIfFalse, next_arm, *span);
                self.emit_op(Opcode::LoadLocal);
                self.emit_u32(scrut_slot);
                self.emit_expr(hi_value)?;
                self.emit_op(if *inclusive { Opcode::Le } else { Opcode::Lt });
                self.emit_jump(Opcode::JumpIfFalse, next_arm, *span);
                Ok(())
            }
            Pattern::Or { alts, span } => {
                // Or-pattern: any alt may succeed. First-cut policy
                // disallows identifier bindings inside alternatives
                // because the v0 emitter has no machinery yet for
                // proving every alternative binds the same set of
                // names with compatible types. Refining this lands
                // alongside the type-checker slice.
                for alt in alts {
                    if matches!(alt, Pattern::Ident(_)) {
                        return Err(EmitError::new(
                            EmitErrorKind::UnsupportedFeature {
                                what: "identifier binding in or-pattern",
                            },
                            alt.span(),
                        ));
                    }
                }
                // Successful alts converge at `body_label`, which
                // sits immediately after the last alt's test. Non-
                // last alts: on success Jump body_label; on failure
                // fall through to next_alt. Last alt: on failure
                // jump to caller's next_arm; on success fall through
                // naturally to body_label.
                let body_label = self.new_label();
                let n = alts.len();
                for (i, alt) in alts.iter().enumerate() {
                    if i + 1 == n {
                        self.emit_pattern_test(alt, scrut_slot, next_arm)?;
                    } else {
                        let next_alt = self.new_label();
                        self.emit_pattern_test(alt, scrut_slot, next_alt)?;
                        self.emit_jump(Opcode::Jump, body_label, *span);
                        self.mark_label(next_alt);
                    }
                }
                self.mark_label(body_label);
                Ok(())
            }
            // Pattern kinds whose lowering is not part of this slice
            // yet: report a typed error so the rest of the program
            // still emits and the diagnostic points at the offending
            // arm.
            Pattern::TupleStruct { span, .. } => Err(EmitError::new(
                EmitErrorKind::UnsupportedFeature {
                    what: "tuple-struct pattern in match",
                },
                *span,
            )),
            Pattern::Struct { span, .. } => Err(EmitError::new(
                EmitErrorKind::UnsupportedFeature {
                    what: "struct pattern in match",
                },
                *span,
            )),
            Pattern::Path { span, .. } => Err(EmitError::new(
                EmitErrorKind::UnsupportedFeature {
                    what: "path pattern in match",
                },
                *span,
            )),
            Pattern::Rest { span } => Err(EmitError::new(
                EmitErrorKind::UnsupportedFeature {
                    what: "rest pattern in match",
                },
                *span,
            )),
            Pattern::Error { span } => {
                Err(EmitError::new(EmitErrorKind::ParseErrorInExpr, *span))
            }
        }
    }

    /// Helper: extract the literal expression from a `Pattern::Literal`
    /// endpoint of a range pattern. Range endpoints must be literal
    /// patterns; anything else surfaces a typed `UnsupportedFeature`
    /// so an emitter that runs against a future relaxed grammar
    /// cannot silently emit incorrect code.
    fn pattern_as_literal_value(p: &Pattern) -> Result<&Expr, EmitError> {
        match p {
            Pattern::Literal { value, .. } => Ok(value),
            other => Err(EmitError::new(
                EmitErrorKind::UnsupportedFeature {
                    what: "non-literal range endpoint",
                },
                other.span(),
            )),
        }
    }

    /// Binds pattern identifiers to fresh locals so the arm body can
    /// reference them by name. Only `Pattern::Ident` introduces a
    /// binding in the first cut; other supported patterns
    /// (`Wildcard`, `Literal`) introduce no bindings.
    fn emit_pattern_bindings(
        &mut self,
        pattern: &Pattern,
        scrut_slot: u32,
    ) -> Result<(), EmitError> {
        if let Pattern::Ident(id) = pattern {
            let binding_slot = self.allocate_local(&id.name, id.span)?;
            self.emit_op(Opcode::LoadLocal);
            self.emit_u32(scrut_slot);
            self.emit_op(Opcode::StoreLocal);
            self.emit_u32(binding_slot);
        }
        Ok(())
    }

    fn emit_return(&mut self, value: Option<&Expr>) -> Result<(), EmitError> {
        if let Some(v) = value {
            self.emit_expr(v)?;
        } else {
            self.emit_op(Opcode::LoadNone);
        }
        self.emit_op(Opcode::Return);
        Ok(())
    }
}

/// Parses an integer literal text (decimal, `0x`, `0b`, `0o`, with
/// underscore separators) into an `i64`.
fn parse_int_literal(text: &str) -> Option<i64> {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    if let Some(rest) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        i64::from_str_radix(rest, 16).ok()
    } else if let Some(rest) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) {
        i64::from_str_radix(rest, 2).ok()
    } else if let Some(rest) = clean.strip_prefix("0o").or_else(|| clean.strip_prefix("0O")) {
        i64::from_str_radix(rest, 8).ok()
    } else {
        clean.parse::<i64>().ok()
    }
}

/// Parses a float literal text (decimal, with `e` exponent, with
/// underscore separators) into an `f64`.
fn parse_float_literal(text: &str) -> Option<f64> {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    clean.parse::<f64>().ok()
}

/// Decodes a string literal `"..."` into its runtime string value.
///
/// S5b.1 supports the minimal escape set `\n`, `\t`, `\r`, `\\`, `\"`,
/// `\0`. Other backslash sequences are rejected so the emitter remains
/// fail-closed; a richer escape grammar will land alongside the lexer
/// extension in a future slice.
fn parse_str_literal(text: &str) -> Result<String, &'static str> {
    if text.len() < 2 || !text.starts_with('"') || !text.ends_with('"') {
        return Err("string literal must be quoted");
    }
    let inner = &text[1..text.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('0') => out.push('\0'),
                Some(_) | None => return Err("unsupported escape sequence"),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_decimal() {
        assert_eq!(parse_int_literal("42"), Some(42));
        assert_eq!(parse_int_literal("1_000_000"), Some(1_000_000));
    }

    #[test]
    fn parse_int_hex_bin_oct() {
        assert_eq!(parse_int_literal("0xFF"), Some(255));
        assert_eq!(parse_int_literal("0b1010"), Some(10));
        assert_eq!(parse_int_literal("0o17"), Some(15));
    }

    #[test]
    fn parse_int_rejects_garbage() {
        assert_eq!(parse_int_literal("abc"), None);
        assert_eq!(parse_int_literal("0xZZ"), None);
    }

    #[test]
    fn parse_float_basic() {
        assert_eq!(parse_float_literal("3.14"), Some(3.14));
        assert_eq!(parse_float_literal("1_000.5"), Some(1000.5));
        assert_eq!(parse_float_literal("1e9"), Some(1e9));
    }

    #[test]
    fn parse_str_no_escapes() {
        assert_eq!(parse_str_literal("\"hello\""), Ok("hello".to_string()));
    }

    #[test]
    fn parse_str_with_escapes() {
        assert_eq!(
            parse_str_literal("\"a\\nb\\tc\""),
            Ok("a\nb\tc".to_string())
        );
    }

    #[test]
    fn parse_str_rejects_unknown_escape() {
        assert!(parse_str_literal("\"\\q\"").is_err());
    }

    #[test]
    fn const_pool_dedups() {
        let mut p = ConstPoolBuilder::default();
        assert_eq!(p.intern_int(7), 0);
        assert_eq!(p.intern_int(7), 0);
        assert_eq!(p.intern_int(8), 1);
        assert_eq!(p.entries.len(), 2);
    }

    #[test]
    fn const_pool_float_dedup_uses_bits() {
        let mut p = ConstPoolBuilder::default();
        // Distinct values, distinct entries.
        assert_eq!(p.intern_float(1.0), 0);
        assert_eq!(p.intern_float(2.0), 1);
        // Identical value, same entry.
        assert_eq!(p.intern_float(1.0), 0);
        // -0.0 has a distinct bit pattern from 0.0 so they get separate
        // entries, matching the constant-pool encoder's behaviour.
        assert_eq!(p.intern_float(0.0), 2);
        assert_eq!(p.intern_float(-0.0), 3);
    }
}
