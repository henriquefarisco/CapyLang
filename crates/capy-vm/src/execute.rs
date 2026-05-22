//! Loader + interpreter loop for the v0 VM.
//!
//! Loading: [`Vm::from_module`] parses the bytecode container, decodes
//! the constant pool and function table, decodes each function's
//! instruction stream up-front, and pre-computes a byte-offset → index
//! map so jump targets can be resolved in O(1).
//!
//! Execution: [`Vm::run`] / [`Vm::run_with_budget`] sets up a fresh
//! evaluation stack and an initial call frame for the requested
//! entry-point and interprets one instruction at a time, decrementing
//! the budget on every step. `Call` pushes a new frame (with a fresh
//! locals window seeded from the caller's operand stack) and `Return`
//! pops it, restoring the caller. The interpreter never panics: every
//! malformed condition (stack underflow, out-of-bounds index, type
//! mismatch, division by zero, jump to a non-instruction boundary,
//! budget exhausted, call-depth overflow, arity mismatch) maps to a
//! deterministic [`VmError`].

#![forbid(unsafe_code)]

use std::collections::HashMap;

use capy_bytecode::{
    decode, verify_function, ConstPool, Constant, FunctionTable, Import, ImportTable, Instruction,
    Module, SectionTag, VerifyError,
};

use crate::host::HostAdapter;

use crate::error::VmError;
use crate::value::Value;

/// Default instruction budget for [`Vm::run`]. Generous enough for
/// host-test programs; tighten via [`Vm::run_with_budget`] when
/// modelling CapyOS per-frame budgets.
pub const DEFAULT_INSTRUCTION_BUDGET: u64 = 1_000_000;

/// Maximum number of nested `Call` frames the VM keeps live at once.
///
/// Picked to be generous for hand-written CapyLang programs while still
/// trapping unbounded recursion deterministically and well below any
/// realistic native-stack limit.
pub const MAX_CALL_DEPTH: usize = 256;

/// A loaded, ready-to-execute module.
#[derive(Debug)]
pub struct Vm {
    consts: Vec<Constant>,
    functions: Vec<CompiledFunction>,
    by_name: HashMap<String, usize>,
    imports: Vec<Import>,
    host: HostAdapter,
}

#[derive(Debug)]
struct CompiledFunction {
    name: String,
    locals_count: u32,
    instructions: Vec<Instruction>,
    /// Byte offset of each instruction inside the original `code`
    /// slice; `byte_offsets.len() == instructions.len()`.
    byte_offsets: Vec<u32>,
    /// Maps a byte offset back to the instruction index. The map
    /// excludes the "past end" sentinel so jumping past the last
    /// instruction surfaces as [`VmError::JumpOutOfBounds`].
    offset_to_index: HashMap<u32, usize>,
}

impl Vm {
    /// Parses a serialized v0 [`Module`] and prepares it for execution
    /// with an empty [`HostAdapter`]. Any `HostCall` opcode in the
    /// module will trap with [`VmError::UnresolvedHostImport`] until a
    /// non-empty adapter is supplied via
    /// [`Vm::from_module_with_host`] or [`Vm::with_host_adapter`].
    pub fn from_module(bytes: &[u8]) -> Result<Self, VmError> {
        Self::from_module_with_host(bytes, HostAdapter::new())
    }

    /// Parses a serialized v0 [`Module`] and prepares it for execution
    /// with the provided [`HostAdapter`]. `HostCall` opcodes look up
    /// their `(module, symbol)` pair in the adapter; unresolved entries
    /// trap deterministically with [`VmError::UnresolvedHostImport`].
    pub fn from_module_with_host(bytes: &[u8], host: HostAdapter) -> Result<Self, VmError> {
        let module = Module::parse(bytes).map_err(|e| VmError::MalformedModule {
            reason: "container header or sections invalid",
            code: e.code(),
        })?;

        let consts = match module.sections.iter().find(|s| s.tag == SectionTag::Consts) {
            Some(section) => {
                ConstPool::decode(&section.payload)
                    .map_err(|e| VmError::MalformedModule {
                        reason: "constant pool payload invalid",
                        code: e.code(),
                    })?
                    .entries
            }
            None => Vec::new(),
        };

        let imports = match module
            .sections
            .iter()
            .find(|s| s.tag == SectionTag::Imports)
        {
            Some(section) => {
                ImportTable::decode(&section.payload)
                    .map_err(|e| VmError::MalformedModule {
                        reason: "import table payload invalid",
                        code: e.code(),
                    })?
                    .entries
            }
            None => Vec::new(),
        };

        let function_table = match module
            .sections
            .iter()
            .find(|s| s.tag == SectionTag::Functions)
        {
            Some(section) => {
                FunctionTable::decode(&section.payload).map_err(|e| VmError::MalformedModule {
                    reason: "function table payload invalid",
                    code: e.code(),
                })?
            }
            None => FunctionTable {
                entries: Vec::new(),
            },
        };

        let mut functions = Vec::with_capacity(function_table.entries.len());
        let mut by_name = HashMap::with_capacity(function_table.entries.len());
        for (i, f) in function_table.entries.into_iter().enumerate() {
            let compiled = compile_function(f)?;
            by_name.insert(compiled.name.clone(), i);
            functions.push(compiled);
        }

        // Static verification pass: every function must satisfy the v0
        // stack-discipline contract before any instruction executes.
        // The verifier needs each callee's declared `locals_count`, so
        // we collect that once across the whole table.
        let callee_locals_counts: Vec<u32> = functions.iter().map(|f| f.locals_count).collect();
        for f in &functions {
            verify_function(&f.instructions, f.locals_count, &callee_locals_counts)
                .map_err(verify_to_vm_error)?;
        }

        Ok(Self {
            consts,
            functions,
            by_name,
            imports,
            host,
        })
    }

    /// Swaps in a new [`HostAdapter`] without re-parsing the module.
    /// Useful for tests that want to register handlers after load and
    /// for the `capyc run` subcommand which wires the built-in stubs.
    #[must_use]
    pub fn with_host_adapter(mut self, host: HostAdapter) -> Self {
        self.host = host;
        self
    }

    /// Runs the named function with the default instruction budget and
    /// returns its top-of-stack return value (or [`Value::None`] if the
    /// function returned without pushing a value).
    pub fn run(&self, fn_name: &str) -> Result<Value, VmError> {
        self.run_with_budget(fn_name, DEFAULT_INSTRUCTION_BUDGET)
    }

    /// Like [`Vm::run`] but with a caller-supplied instruction budget.
    pub fn run_with_budget(&self, fn_name: &str, budget: u64) -> Result<Value, VmError> {
        let idx = self
            .by_name
            .get(fn_name)
            .copied()
            .ok_or_else(|| VmError::UnknownFunction {
                name: fn_name.to_string(),
            })?;
        let entry = &self.functions[idx];
        let cur = Frame {
            func_idx: idx,
            i: 0,
            locals: vec![Value::None; entry.locals_count as usize],
        };
        let mut state = ExecState {
            stack: Vec::new(),
            frames: Vec::new(),
            cur,
            budget,
            consts: &self.consts,
            functions: &self.functions,
            imports: &self.imports,
            host: &self.host,
        };
        state.run()
    }
}

/// One live call frame. `i` is the instruction index inside
/// `functions[func_idx]`; `locals` is the frame-local variable window.
#[derive(Debug)]
struct Frame {
    func_idx: usize,
    i: usize,
    locals: Vec<Value>,
}

struct ExecState<'a> {
    /// Shared operand stack across all frames. `Call` reads its
    /// arguments from the top of this stack and `Return` pushes the
    /// callee's return value back.
    stack: Vec<Value>,
    /// Suspended caller frames, deepest at the bottom.
    frames: Vec<Frame>,
    /// Currently executing frame.
    cur: Frame,
    budget: u64,
    consts: &'a [Constant],
    functions: &'a [CompiledFunction],
    imports: &'a [Import],
    host: &'a HostAdapter,
}

impl<'a> ExecState<'a> {
    fn run(&mut self) -> Result<Value, VmError> {
        loop {
            let func = &self.functions[self.cur.func_idx];

            // Falling off the end of the instruction stream without an
            // explicit `Return` is treated like an implicit `return ()`.
            if self.cur.i >= func.instructions.len() {
                let v = self.stack.pop().unwrap_or(Value::None);
                if let Some(prev) = self.frames.pop() {
                    self.cur = prev;
                    self.stack.push(v);
                    continue;
                }
                return Ok(v);
            }

            if self.budget == 0 {
                return Err(VmError::BudgetExhausted { budget: 0 });
            }
            self.budget -= 1;

            let ins = func.instructions[self.cur.i];
            let pc = func.byte_offsets[self.cur.i];

            match ins {
                Instruction::Nop => {}
                Instruction::Pop => {
                    self.pop(pc)?;
                }
                Instruction::LoadConst(idx) => {
                    let c = self
                        .consts
                        .get(idx as usize)
                        .ok_or(VmError::ConstOutOfBounds {
                            pc,
                            index: idx,
                            pool_len: self.consts.len() as u32,
                        })?;
                    let v = match c {
                        Constant::Int(v) => Value::Int(*v),
                        Constant::Float(v) => Value::Float(*v),
                        Constant::Str(s) => Value::Str(s.clone()),
                    };
                    self.stack.push(v);
                }
                Instruction::LoadTrue => self.stack.push(Value::Bool(true)),
                Instruction::LoadFalse => self.stack.push(Value::Bool(false)),
                Instruction::LoadNone => self.stack.push(Value::None),
                Instruction::LoadLocal(idx) => {
                    let v = self.cur.locals.get(idx as usize).cloned().ok_or(
                        VmError::LocalOutOfBounds {
                            pc,
                            index: idx,
                            locals_count: self.cur.locals.len() as u32,
                        },
                    )?;
                    self.stack.push(v);
                }
                Instruction::StoreLocal(idx) => {
                    let v = self.pop(pc)?;
                    let locals_count = self.cur.locals.len() as u32;
                    let slot =
                        self.cur
                            .locals
                            .get_mut(idx as usize)
                            .ok_or(VmError::LocalOutOfBounds {
                                pc,
                                index: idx,
                                locals_count,
                            })?;
                    *slot = v;
                }
                Instruction::Add => self.binop_numeric(pc, "add", BinOp::Add)?,
                Instruction::Sub => self.binop_numeric(pc, "sub", BinOp::Sub)?,
                Instruction::Mul => self.binop_numeric(pc, "mul", BinOp::Mul)?,
                Instruction::Div => self.binop_numeric(pc, "div", BinOp::Div)?,
                Instruction::Mod => self.binop_numeric(pc, "mod", BinOp::Mod)?,
                Instruction::Neg => {
                    let v = self.pop(pc)?;
                    let r = match v {
                        Value::Int(x) => Value::Int(x.wrapping_neg()),
                        Value::Float(x) => Value::Float(-x),
                        other => {
                            return Err(VmError::TypeMismatch {
                                pc,
                                op: "neg",
                                expected: "int|float",
                                found: other.type_name(),
                            });
                        }
                    };
                    self.stack.push(r);
                }
                Instruction::Eq => self.cmp_equality(pc, "eq", true)?,
                Instruction::Ne => self.cmp_equality(pc, "ne", false)?,
                Instruction::Lt => self.cmp_order(pc, "lt", Ordering::Lt)?,
                Instruction::Le => self.cmp_order(pc, "le", Ordering::Le)?,
                Instruction::Gt => self.cmp_order(pc, "gt", Ordering::Gt)?,
                Instruction::Ge => self.cmp_order(pc, "ge", Ordering::Ge)?,
                Instruction::Not => {
                    let v = self.pop(pc)?;
                    match v {
                        Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                        other => {
                            return Err(VmError::TypeMismatch {
                                pc,
                                op: "not",
                                expected: "bool",
                                found: other.type_name(),
                            });
                        }
                    }
                }
                Instruction::Jump(offset) => {
                    let after_imm = pc as i64 + ins.width() as i64;
                    let target = after_imm + offset as i64;
                    self.cur.i = resolve_jump(func, target, pc)?;
                    continue;
                }
                Instruction::JumpIfFalse(offset) => {
                    let v = self.pop(pc)?;
                    let cond = match v {
                        Value::Bool(b) => b,
                        other => {
                            return Err(VmError::ExpectedBool {
                                pc,
                                found: other.type_name(),
                            });
                        }
                    };
                    if !cond {
                        let after_imm = pc as i64 + ins.width() as i64;
                        let target = after_imm + offset as i64;
                        self.cur.i = resolve_jump(func, target, pc)?;
                        continue;
                    }
                }
                Instruction::Call { fn_idx, argc } => {
                    let callee_idx = fn_idx as usize;
                    if callee_idx >= self.functions.len() {
                        return Err(VmError::UnknownFunctionIndex {
                            pc,
                            index: fn_idx,
                            table_len: self.functions.len() as u32,
                        });
                    }
                    // Snapshot the callee's identity up-front so later
                    // mutations of `self.cur` / `self.frames` cannot
                    // interfere with the borrow of `self.functions`.
                    let (callee_locals_count, callee_name) = {
                        let callee = &self.functions[callee_idx];
                        (callee.locals_count, callee.name.clone())
                    };
                    if argc > callee_locals_count {
                        return Err(VmError::CallArityMismatch {
                            pc,
                            callee: callee_name,
                            argc,
                            locals_count: callee_locals_count,
                        });
                    }
                    if self.stack.len() < argc as usize {
                        return Err(VmError::StackUnderflow { pc });
                    }
                    // Live frames after the push: frames + cur + new = frames.len() + 2.
                    // Trap when that would exceed MAX_CALL_DEPTH.
                    if self.frames.len() + 2 > MAX_CALL_DEPTH {
                        return Err(VmError::CallStackOverflow {
                            pc,
                            depth: MAX_CALL_DEPTH,
                        });
                    }
                    let split = self.stack.len() - argc as usize;
                    let args: Vec<Value> = self.stack.split_off(split);
                    let mut locals = vec![Value::None; callee_locals_count as usize];
                    for (i, v) in args.into_iter().enumerate() {
                        locals[i] = v;
                    }
                    // Advance the caller's PC past the `Call` so the
                    // restored frame resumes on the next instruction.
                    self.cur.i += 1;
                    let new_frame = Frame {
                        func_idx: callee_idx,
                        i: 0,
                        locals,
                    };
                    let prev = std::mem::replace(&mut self.cur, new_frame);
                    self.frames.push(prev);
                    continue;
                }
                Instruction::Return => {
                    let v = self.stack.pop().unwrap_or(Value::None);
                    if let Some(prev) = self.frames.pop() {
                        self.cur = prev;
                        self.stack.push(v);
                        continue;
                    }
                    return Ok(v);
                }
                Instruction::HostCall { import_idx, argc } => {
                    let table_len = self.imports.len() as u32;
                    if import_idx >= table_len {
                        return Err(VmError::UnknownHostImport {
                            pc,
                            index: import_idx,
                            table_len,
                        });
                    }
                    if self.stack.len() < argc as usize {
                        return Err(VmError::StackUnderflow { pc });
                    }
                    let import = &self.imports[import_idx as usize];
                    let handler = self
                        .host
                        .lookup(&import.module, &import.symbol)
                        .ok_or_else(|| VmError::UnresolvedHostImport {
                            pc,
                            module: import.module.clone(),
                            symbol: import.symbol.clone(),
                        })?;
                    let split = self.stack.len() - argc as usize;
                    let args: Vec<Value> = self.stack.split_off(split);
                    match handler(&args) {
                        Ok(v) => self.stack.push(v),
                        Err(reason) => {
                            return Err(VmError::HostCallFailed {
                                pc,
                                module: import.module.clone(),
                                symbol: import.symbol.clone(),
                                reason,
                            });
                        }
                    }
                }
            }
            self.cur.i += 1;
        }
    }

    fn pop(&mut self, pc: u32) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow { pc })
    }

    fn binop_numeric(&mut self, pc: u32, op: &'static str, kind: BinOp) -> Result<(), VmError> {
        let b = self.pop(pc)?;
        let a = self.pop(pc)?;
        let r = match (a, b) {
            (Value::Int(x), Value::Int(y)) => match kind {
                BinOp::Add => Value::Int(x.wrapping_add(y)),
                BinOp::Sub => Value::Int(x.wrapping_sub(y)),
                BinOp::Mul => Value::Int(x.wrapping_mul(y)),
                BinOp::Div => {
                    if y == 0 {
                        return Err(VmError::DivisionByZero { pc });
                    }
                    Value::Int(x.wrapping_div(y))
                }
                BinOp::Mod => {
                    if y == 0 {
                        return Err(VmError::DivisionByZero { pc });
                    }
                    Value::Int(x.wrapping_rem(y))
                }
            },
            (Value::Float(x), Value::Float(y)) => Value::Float(apply_float(kind, x, y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(apply_float(kind, x as f64, y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(apply_float(kind, x, y as f64)),
            (a, b) => {
                return Err(VmError::TypeMismatch {
                    pc,
                    op,
                    expected: "int|float",
                    found: type_pair(&a, &b),
                });
            }
        };
        self.stack.push(r);
        Ok(())
    }

    fn cmp_equality(&mut self, pc: u32, op: &'static str, want_eq: bool) -> Result<(), VmError> {
        let b = self.pop(pc)?;
        let a = self.pop(pc)?;
        let r = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => *x == *y,
            (Value::Float(x), Value::Float(y)) => *x == *y,
            (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
            (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
            (Value::Bool(x), Value::Bool(y)) => *x == *y,
            (Value::None, Value::None) => true,
            (Value::Str(x), Value::Str(y)) => x == y,
            _ => {
                return Err(VmError::TypeMismatch {
                    pc,
                    op,
                    expected: "matching primitive types",
                    found: type_pair(&a, &b),
                });
            }
        };
        self.stack.push(Value::Bool(if want_eq { r } else { !r }));
        Ok(())
    }

    fn cmp_order(&mut self, pc: u32, op: &'static str, kind: Ordering) -> Result<(), VmError> {
        let b = self.pop(pc)?;
        let a = self.pop(pc)?;
        let r = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => apply_ord_i64(kind, *x, *y),
            (Value::Float(x), Value::Float(y)) => apply_ord_f64(kind, *x, *y),
            (Value::Int(x), Value::Float(y)) => apply_ord_f64(kind, *x as f64, *y),
            (Value::Float(x), Value::Int(y)) => apply_ord_f64(kind, *x, *y as f64),
            _ => {
                return Err(VmError::TypeMismatch {
                    pc,
                    op,
                    expected: "int|float",
                    found: type_pair(&a, &b),
                });
            }
        };
        self.stack.push(Value::Bool(r));
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, Copy)]
enum Ordering {
    Lt,
    Le,
    Gt,
    Ge,
}

fn apply_float(kind: BinOp, x: f64, y: f64) -> f64 {
    match kind {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div => x / y,
        BinOp::Mod => x % y,
    }
}

fn apply_ord_i64(kind: Ordering, x: i64, y: i64) -> bool {
    match kind {
        Ordering::Lt => x < y,
        Ordering::Le => x <= y,
        Ordering::Gt => x > y,
        Ordering::Ge => x >= y,
    }
}

fn apply_ord_f64(kind: Ordering, x: f64, y: f64) -> bool {
    match kind {
        Ordering::Lt => x < y,
        Ordering::Le => x <= y,
        Ordering::Gt => x > y,
        Ordering::Ge => x >= y,
    }
}

fn type_pair(a: &Value, b: &Value) -> &'static str {
    // Compress the (a, b) pair into a single static string so the
    // error remains `Copy`/`'static`-friendly without allocation.
    match (a.type_name(), b.type_name()) {
        ("int", "int") => "int,int",
        ("int", "float") => "int,float",
        ("int", "bool") => "int,bool",
        ("int", "str") => "int,str",
        ("int", "none") => "int,none",
        ("float", "int") => "float,int",
        ("float", "float") => "float,float",
        ("float", "bool") => "float,bool",
        ("float", "str") => "float,str",
        ("float", "none") => "float,none",
        ("bool", "int") => "bool,int",
        ("bool", "float") => "bool,float",
        ("bool", "bool") => "bool,bool",
        ("bool", "str") => "bool,str",
        ("bool", "none") => "bool,none",
        ("str", "int") => "str,int",
        ("str", "float") => "str,float",
        ("str", "bool") => "str,bool",
        ("str", "str") => "str,str",
        ("str", "none") => "str,none",
        ("none", "int") => "none,int",
        ("none", "float") => "none,float",
        ("none", "bool") => "none,bool",
        ("none", "str") => "none,str",
        ("none", "none") => "none,none",
        _ => "?,?",
    }
}

fn resolve_jump(func: &CompiledFunction, target: i64, pc: u32) -> Result<usize, VmError> {
    if target < 0 || target > u32::MAX as i64 {
        return Err(VmError::JumpOutOfBounds { pc, target });
    }
    let t = target as u32;
    func.offset_to_index
        .get(&t)
        .copied()
        .ok_or(VmError::JumpOutOfBounds { pc, target })
}

/// Map a [`VerifyError`] to [`VmError::MalformedModule`], preserving
/// the verifier's stable diagnostic code so downstream tooling can
/// branch on it without depending on `capy-bytecode` internals.
fn verify_to_vm_error(e: VerifyError) -> VmError {
    let reason: &'static str = match e {
        VerifyError::StackUnderflow { .. } => "function violates stack discipline (underflow)",
        VerifyError::StackInconsistency { .. } => {
            "function violates stack discipline (path-disagreement on stack depth)"
        }
        VerifyError::FallOffEnd { .. } => "function falls off the end without Return",
        VerifyError::InvalidReturnDepth { .. } => "Return reached with non-unit operand stack",
        VerifyError::LocalOutOfBounds { .. } => "function references an out-of-bounds local slot",
        VerifyError::JumpOutOfBounds { .. } => "function jumps outside its instruction stream",
        VerifyError::UnknownFunctionIndex { .. } => "Call references an unknown function index",
        VerifyError::CallArityOverflow { .. } => "Call argc exceeds the callee's locals window",
    };
    VmError::MalformedModule {
        reason,
        code: e.code(),
    }
}

fn compile_function(f: capy_bytecode::Function) -> Result<CompiledFunction, VmError> {
    let instructions = decode(&f.code).map_err(|e| VmError::MalformedModule {
        reason: "function instruction stream invalid",
        code: e.code(),
    })?;
    let mut byte_offsets = Vec::with_capacity(instructions.len());
    let mut off: u32 = 0;
    for ins in &instructions {
        byte_offsets.push(off);
        off = off
            .checked_add(ins.width() as u32)
            .ok_or(VmError::MalformedModule {
                reason: "function instruction stream too large",
                code: "B0009",
            })?;
    }
    let mut offset_to_index = HashMap::with_capacity(instructions.len());
    for (idx, &b) in byte_offsets.iter().enumerate() {
        offset_to_index.insert(b, idx);
    }
    Ok(CompiledFunction {
        name: f.name,
        locals_count: f.locals_count,
        instructions,
        byte_offsets,
        offset_to_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use capy_bytecode::{
        encode, Function, FunctionTable, Import, ImportTable, Module, Section, SectionTag,
    };

    fn module_with(consts: Vec<Constant>, fns: Vec<Function>) -> Vec<u8> {
        let consts_payload = ConstPool { entries: consts }.encode();
        let functions_payload = FunctionTable { entries: fns }.encode();
        Module::new(
            0,
            vec![
                Section::new(SectionTag::Consts, consts_payload),
                Section::new(SectionTag::Functions, functions_payload),
            ],
        )
        .serialize()
    }

    #[test]
    fn add_two_int_constants() {
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![Constant::Int(2), Constant::Int(3)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        assert_eq!(vm.run("main").unwrap(), Value::Int(5));
    }

    #[test]
    fn unknown_function_traps() {
        let bytes = module_with(vec![], vec![]);
        let vm = Vm::from_module(&bytes).unwrap();
        let err = vm.run("missing").unwrap_err();
        match err {
            VmError::UnknownFunction { name } => assert_eq!(name, "missing"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn division_by_zero_traps() {
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Div,
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![Constant::Int(10), Constant::Int(0)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::DivisionByZero { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn integer_overflow_wraps() {
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![Constant::Int(i64::MAX), Constant::Int(1)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        assert_eq!(vm.run("main").unwrap(), Value::Int(i64::MIN));
    }

    #[test]
    fn budget_exhaustion_traps() {
        // Minimal function: LoadNone; Return. Two instructions.
        let code = encode(&[Instruction::LoadNone, Instruction::Return]);
        let bytes = module_with(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        // Budget = 1 is enough for LoadNone but not Return.
        match vm.run_with_budget("main", 1).unwrap_err() {
            VmError::BudgetExhausted { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
        // Budget = 2 succeeds.
        assert_eq!(vm.run_with_budget("main", 2).unwrap(), Value::None);
    }

    #[test]
    fn type_mismatch_int_bool_traps() {
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::LoadTrue,
            Instruction::Add,
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![Constant::Int(1)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::TypeMismatch { op, .. } => assert_eq!(op, "add"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn jump_if_false_requires_bool() {
        // Hand-crafted (verifier-clean) sequence whose JIF cond is an
        // int rather than a bool: the bytecode is statically balanced
        // (every reachable path lands at `Return` with depth 1) so the
        // load-time verifier accepts it, but the JIF pops an `Int(0)`
        // at runtime and traps with `ExpectedBool`.
        //
        //   i=0 LoadConst(0)        ; push Int(0)         (5 bytes)
        //   i=1 JumpIfFalse(+0)     ; fall through to i=2 (5 bytes)
        //   i=2 LoadNone            ; push None           (1 byte)
        //   i=3 Return
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::JumpIfFalse(0),
            Instruction::LoadNone,
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![Constant::Int(0)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        let vm = Vm::from_module(&bytes).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::ExpectedBool { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn call_unknown_function_index_rejected_at_load() {
        // The static verifier catches an out-of-bounds `fn_idx` at
        // `from_module` time. The VM runtime never sees this
        // bytecode — surfaces as `MalformedModule { code: B0019 }`.
        let code = encode(&[
            Instruction::Call { fn_idx: 7, argc: 0 },
            Instruction::Return,
        ]);
        let bytes = module_with(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
        );
        match Vm::from_module(&bytes).unwrap_err() {
            VmError::MalformedModule { code, .. } => {
                assert_eq!(code, capy_bytecode::B_VERIFIER_UNKNOWN_FUNCTION_INDEX);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn call_arity_overflow_rejected_at_load() {
        // Helper has locals_count=0 but caller passes argc=1. The
        // verifier rejects at load time with `B0020`.
        let main_code = encode(&[
            Instruction::LoadConst(0),
            Instruction::Call { fn_idx: 1, argc: 1 },
            Instruction::Return,
        ]);
        let helper_code = encode(&[Instruction::LoadNone, Instruction::Return]);
        let bytes = module_with(
            vec![Constant::Int(1)],
            vec![
                Function {
                    name: "main".into(),
                    locals_count: 0,
                    code: main_code,
                },
                Function {
                    name: "helper".into(),
                    locals_count: 0,
                    code: helper_code,
                },
            ],
        );
        match Vm::from_module(&bytes).unwrap_err() {
            VmError::MalformedModule { code, .. } => {
                assert_eq!(code, capy_bytecode::B_VERIFIER_CALL_ARITY_OVERFLOW);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    // --- S7: host bridge --------------------------------------------------

    fn module_with_imports(
        consts: Vec<Constant>,
        fns: Vec<Function>,
        imports: Vec<Import>,
    ) -> Vec<u8> {
        let consts_payload = ConstPool { entries: consts }.encode();
        let functions_payload = FunctionTable { entries: fns }.encode();
        let imports_payload = ImportTable { entries: imports }.encode();
        Module::new(
            0,
            vec![
                Section::new(SectionTag::Consts, consts_payload),
                Section::new(SectionTag::Imports, imports_payload),
                Section::new(SectionTag::Functions, functions_payload),
            ],
        )
        .serialize()
    }

    #[test]
    fn host_call_dispatches_to_registered_stub() {
        let code = encode(&[
            Instruction::HostCall {
                import_idx: 0,
                argc: 0,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![Import {
                module: "time".into(),
                symbol: "now".into(),
            }],
        );
        let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs()).unwrap();
        assert_eq!(vm.run("main").unwrap(), Value::Int(0));
    }

    #[test]
    fn host_call_unknown_import_idx_traps() {
        let code = encode(&[
            Instruction::HostCall {
                import_idx: 5,
                argc: 0,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![],
        );
        let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs()).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::UnknownHostImport {
                index, table_len, ..
            } => {
                assert_eq!(index, 5);
                assert_eq!(table_len, 0);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn host_call_unresolved_symbol_traps() {
        let code = encode(&[
            Instruction::HostCall {
                import_idx: 0,
                argc: 0,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![Import {
                module: "time".into(),
                symbol: "missing".into(),
            }],
        );
        let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs()).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::UnresolvedHostImport { module, symbol, .. } => {
                assert_eq!(module, "time");
                assert_eq!(symbol, "missing");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn host_call_with_arg_forwards_to_log_info() {
        // load_const 0 ("hi"); host_call import 0 with argc=1; return
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::HostCall {
                import_idx: 0,
                argc: 1,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![Constant::Str("hi".into())],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![Import {
                module: "log".into(),
                symbol: "info".into(),
            }],
        );
        let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs()).unwrap();
        assert_eq!(vm.run("main").unwrap(), Value::None);
    }

    #[test]
    fn host_call_handler_error_is_surfaced() {
        // log::info with a non-string argument must produce
        // HostCallFailed carrying the static reason verbatim.
        let code = encode(&[
            Instruction::LoadConst(0),
            Instruction::HostCall {
                import_idx: 0,
                argc: 1,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![Constant::Int(42)],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![Import {
                module: "log".into(),
                symbol: "info".into(),
            }],
        );
        let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs()).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::HostCallFailed {
                module,
                symbol,
                reason,
                ..
            } => {
                assert_eq!(module, "log");
                assert_eq!(symbol, "info");
                assert_eq!(reason, "log::info expects a Str argument");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn host_call_without_adapter_traps_deterministically() {
        let code = encode(&[
            Instruction::HostCall {
                import_idx: 0,
                argc: 0,
            },
            Instruction::Return,
        ]);
        let bytes = module_with_imports(
            vec![],
            vec![Function {
                name: "main".into(),
                locals_count: 0,
                code,
            }],
            vec![Import {
                module: "time".into(),
                symbol: "now".into(),
            }],
        );
        // Default adapter is empty; the import must trap with
        // UnresolvedHostImport, never panic.
        let vm = Vm::from_module(&bytes).unwrap();
        match vm.run("main").unwrap_err() {
            VmError::UnresolvedHostImport { module, symbol, .. } => {
                assert_eq!(module, "time");
                assert_eq!(symbol, "now");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
