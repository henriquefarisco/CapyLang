# CapyLang compatibility and integration contract

CapyLang owns the **portable language-core logic** (lexer, parser,
bytecode/IR, VM, minimal stdlib, mock host ABI). CapyLang modules
must remain portable and must run through a versioned host ABI when
integrated with CapyOS.

## CapyOS reference version

- CapyOS core pinned for this contract: `0.8.0-alpha.262+20260602`
- Authoritative cross-repo matrix: [`CapyOS/docs/reference/integration/compatibility-matrix.md`](../../CapyOS/docs/reference/integration/compatibility-matrix.md)
- Canonical manifest format consumed by the in-tree adapter: [`CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`](../../CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md)
- Manual deploy runbook: [`CapyOS/docs/operations/manual-module-deploy-runbook.md`](../../CapyOS/docs/operations/manual-module-deploy-runbook.md)
- Current cross-repo audit: [`CapyOS/docs/reference/integration/compatibility-audit-2026-06-02.md`](../../CapyOS/docs/reference/integration/compatibility-audit-2026-06-02.md)

Authoritative CapyOS references:

- `CapyOS/docs/reference/integration/modular-installation-architecture.md`
- `CapyOS/docs/reference/integration/capylang-integration-contract.md`
- `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`
- `CapyOS/docs/reference/integration/external-core-repositories.md`

Local detailed contracts:

- `docs/integration.md` — boundary, required artifacts, host ABI modules, security rules.
- `docs/bytecode-v0.md` — bytecode container format (header frozen by S4).
- `docs/lexer.md` — lexer slice S1 contract (delivered).
- `docs/grammar.ebnf` — lexical + planned syntactic grammar.

## Owned ABI

CapyLang shares ownership of the `capy-lang-host` ABI (v0 partial)
with CapyOS. The current delivered slice is the lexer (`capy-lexer`
crate); parser, bytecode, VM and host ABI bindings are roadmap.

Current ABI surface:

- lexer (S1) — public `Lexer`, `Token`, `TokenKind`, `Span`,
  `Diagnostic`, `LexErrorKind`, `tokenize()`, predicates
  `carries_text` and `is_trivia`, canonical dump format.
- AST + parser (S2.0, expression sublanguage) — public `Expr` enum
  (`Int`, `Float`, `Str`, `Bool`, `NoneLit`, `Ident`, `Path`, `Paren`,
  `Call`, `Index`, `Field`, `Unary`, `Binary`, `Block`, `If`,
  `While`, `Loop`, `For`, `Return`, `Break`, `Continue`, `Match`,
  `Array`, `Assign`, `StructLit`, `Error`) with the `StructLitField`
  companion (S6.3c),
  the `MatchArm` and `Pattern` (`Wildcard`, `Rest`, `Literal`,
  `Ident`, `Path`, `TupleStruct`, `Struct`, `Or`, `Range`, `Error`)
  + `StructPatternField` companions for S2.2b, `Ident`,
  `UnOp`, `BinOp` (with stable `precedence()` and `as_str()`), the
  canonical `dump_expr` text format, `parse_expr()`,
  `ParseDiagnostic`, `ParseErrorKind` (`UnexpectedToken`,
  `UnexpectedEof`, `Lex`). Trivia is filtered before parsing; lexer
  diagnostics are forwarded as `ParseErrorKind::Lex`.
- Statements, blocks and top-level source (S2.1) — new `Stmt` enum
  (`Let`, `Expr { has_semi }`), `Source { stmts, span }` top-level
  node, `Expr::Block { stmts, tail, span }` variant,
  `Expr::is_block_like()` predicate, `dump_source` canonical text
  format and `parse_source()` entry returning
  `ParseSourceResult { source, diagnostics }`. Missing `;` and missing
  `}` are recoverable (typed diagnostics, no panic).
- Control flow expressions (S2.2) — `Expr::If { cond, then_branch,
  else_branch, span }` (recursive on `else if ...`), `Expr::While`,
  `Expr::Loop`, `Expr::Return { value }`, `Expr::Break { value }`,
  `Expr::Continue`. `If`, `While`, `Loop` are block-like (may stand
  as statements without `;`); `Return`, `Break`, `Continue` are not.
- Item declarations (S2.3a) — `Item` enum (`Fn(FnItem)`,
  `Const(ConstItem)`, `Struct(StructItem)`), supporting types
  `FnItem`, `Param`, `ConstItem`, `StructItem`, `StructField`,
  and the new `Stmt::Item(Item)` variant. Items may interleave with
  `let` / expression statements at the top level (`Source`) and
  inside any `Expr::Block`.
- Type AST + remaining items (S2.3b) — new `Type` enum
  (`Path { segments, span }`, `Error { span }`) replaces the previous
  `Expr` placeholder in every type-annotation slot (`Param.ty`,
  `FnItem.ret_ty`, `ConstItem.ty`, `StructField.ty`, `Stmt::Let.ty`).
  New items `Item::TypeAlias(TypeAlias)`, `Item::Enum(EnumItem)`
  (with `Variant`/`VariantBody { Unit, Tuple(Vec<Type>),
  Struct(Vec<StructField>) }`), `Item::Import(ImportItem)` (path +
  optional `as` alias).
- Structured diagnostics (S3) — `capy-diagnostics` crate publishing
  `Severity`, `Code`, `Label`, `Diagnostic`, `SourceMap`, `render`,
  `bridge::from_lex`, `bridge::from_parse`, `bridge::from_emit`,
  `bridge::from_vm`, `bridge::from_vm_with_debug`. Frozen
  lexer/parser code catalogue:
  `L0001` (`UnterminatedString`), `L0002` (`UnterminatedBlockComment`),
  `L0003` (`UnknownChar`), `P0001` (`UnexpectedToken`),
  `P0002` (`UnexpectedEof`). Bridges thread the emitter `E<NNNN>`
  and VM `V<NNNN>` codes through verbatim using each error's
  `code()` method; the bytecode `B<NNNN>` codes appear via the
  inner code field of `VmError::MalformedModule` (rendered into
  the message). `from_vm` uses an empty primary `Span::new(0, 0)`
  because the VM addresses errors by `pc` rather than source byte
  offset; `from_vm_with_debug` resolves `pc` against the module's
  `DebugInfo` using nearest-not-greater semantics. The emitter
  ships `DebugInfo` for the entry-point function (`functions[0]`)
  whenever it emits at least one instruction; the v0 wire format
  has no function-index field on `DebugEntry`, so resolution is
  accurate inside the entry function and falls back to the empty
  span outside it. A v1 debug section will add the function index.
- Bytecode v0 container (S4) — `capy-bytecode` crate publishing
  `Header` (frozen 32-byte little-endian layout, magic `CAPY`,
  `bc_version`, `abi_version`, `flags` (must be 0 in v0),
  `body_length`, BLAKE3-128 `checksum`), `Section`,
  `SectionTag { Consts=0x01, Functions=0x02, Imports=0x03,
  Debug=0x04 }`, `parse_sections`, `Module`, `compute_checksum`,
  `BytecodeError` (implements `std::fmt::Display` +
  `std::error::Error`, output prefixed with the stable `[B<NNNN>]`
  code) with codes `B0001`-`B0007`. Loader is fail-closed
  (rejects unknown major, non-zero flags, body-length mismatch,
  checksum mismatch, unknown section tag and truncated section
  headers). Serialisation is deterministic.
- Per-section typed encoders/decoders (S4b) — `ConstPool` /
  `Constant { Int(i64), Float(f64), Str(String) }`,
  `FunctionTable` / `Function { name, locals_count, code }`,
  `ImportTable` / `Import { module, symbol }`,
  `DebugInfo` / `DebugEntry { bytecode_offset, source_start,
  source_end }`. Each ships `encode()` / `decode()` and is layered
  on top of the S4 framing — the raw `Section::payload` round-trip
  remains unchanged. Extended error catalogue `B0008`-`B0011`
  covers malformed constant pool, function table, import table and
  debug info payloads.
- VM core (S6.1) — `capy-vm` crate publishing `Vm::from_module`,
  `Vm::from_module_with_host`, `Vm::with_host_adapter`,
  `Vm::run` / `Vm::run_with_budget`, `Value` (`None` / `Bool` /
  `Int(i64)` / `Float(f64)` / `Str(String)` / `Array` /
  `Aggregate`, the last two reference-semantics
  `Rc<RefCell<Vec<Value>>>` handles added by S6.2 (`Array`) and S6.3a
  (`Aggregate { tag, fields }`, a struct instance or enum variant)),
  `HostAdapter`,
  `HostFn`, `HostResult` and `VmError` (implements
  `std::fmt::Display` + `std::error::Error`, output prefixed with
  the stable `[V<NNNN>]` code; exposes `code() -> &'static str` and
  `pc() -> Option<u32>` so downstream tooling can resolve errors
  through `capy-diagnostics::bridge::from_vm_with_debug`) with
  the frozen catalogue
  `V0001`-`V0019` (S5b.3 added `V0011` `CALL_STACK_OVERFLOW`,
  `V0012` `UNKNOWN_FUNCTION_INDEX`, `V0013` `CALL_ARITY_MISMATCH`;
  S7 added `V0014` `UNKNOWN_HOST_IMPORT`, `V0015`
  `UNRESOLVED_HOST_IMPORT`, `V0016` `HOST_CALL_FAILED`; S6.2 added
  `V0017` `INDEX_OUT_OF_BOUNDS`; S6.3a added `V0018`
  `FIELD_OUT_OF_BOUNDS`; S6.2b added `V0019` `POP_EMPTY_ARRAY`)
  (`STACK_UNDERFLOW`, `LOCAL_OUT_OF_BOUNDS`, `CONST_OUT_OF_BOUNDS`,
  `JUMP_OUT_OF_BOUNDS`, `TYPE_MISMATCH`, `DIVISION_BY_ZERO`,
  `BUDGET_EXHAUSTED`, `UNKNOWN_FUNCTION`, `MALFORMED_MODULE`,
  `EXPECTED_BOOL`). Closes the first complete source → tokens →
  AST → bytecode → execution pipeline. Determinism contract:
  wrapping i64 arithmetic, IEEE-754 floats, strict same-type
  binary ops with `Int <-> Float` promotion across the numeric
  category (the `Add` opcode additionally concatenates two `Str`
  operands), strict `Bool` for `JumpIfFalse`/`Not`, deterministic
  error returns, no JIT, no syscalls, no host pointers, no global
  state. Per-call `DEFAULT_INSTRUCTION_BUDGET = 1_000_000` with a
  caller-overridable variant for modelling per-frame budgets.
- AST → bytecode emitter (S5b.1 / S5b.2 / S5b.3 / S7 / S10) — `capy-emitter`
  crate publishing `emit(&Source) -> EmitOutput { module, errors }`,
  `EmitError` and `EmitErrorKind` plus the stable error catalogue
  `E0001`-`E0022` (S5b.2 added `E0014` `BREAK_OUTSIDE_LOOP`,
  `E0015` `CONTINUE_OUTSIDE_LOOP`; S5b.3 added `E0016`
  `UNKNOWN_FUNCTION`, `E0017` `UNSUPPORTED_CALLEE`, `E0018`
  `DUPLICATE_FUNCTION`, `E0019` `TOO_MANY_ARGUMENTS`; S7 added
  `E0020` `DUPLICATE_IMPORT`; S2.4 added `E0021`
  `INVALID_ASSIGN_TARGET`; S10 added `E0022` `METHOD_ARITY`).
  S10 also lowers source-level array methods (`a.push(x)`, `a.pop()`,
  `a.insert(i, x)`, `a.remove(i)`, `a.get(i)`, `a.set(i, x)`, `a.len()`)
  onto the existing `0x60-0x6A` opcodes (no new wire), plus integer
  `min(a, b)` / `max(a, b)` builtins lowered to compare + conditional-jump
  (also no new wire; a user-defined `min` / `max` shadows the builtin),
  plus `clamp(x, lo, hi)` = `max(lo, min(x, hi))` lowered to two such selects
  (no new opcode/const-pool; a user-defined `clamp` shadows it), and the
  single-arg `abs(x)` / `sign(x)` lowered to compare-and-branch selects
  against the literal `0` (`abs` via `Neg`; `sign` yields `-1`/`0`/`1`).
  Lowers the v0 subset of the frontend — literals, locals, paren, unary
  `Neg`/`Not`/`BitNot`, arithmetic + comparison + integer
  bitwise/shift binary operators (`&` `|` `^` `<<` `>>`),
  short-circuit `&&` / `||`, blocks, `let`, assignment to locals and to
  array elements (`target = value` and `a[i] = value`; compound
  `+=`/`-=`/`*=`/`/=`/`%=` desugared in the parser), array literals
  `[e0, ...]` and indexing `a[i]` (S6.2), expr statements,
  `if`/`else`, `while` / `loop` / `for` (integer range) / `break` /
  `continue`, `return`,
  function calls (`Expr::Call` with direct top-level identifier
  callees, lowered to `Call` for in-module `fn` targets and to
  `HostCall` for imported `module::symbol` targets since S7; local
  `fn` items shadow imports of the same callable name) — and the
  top-level `Item::Fn` (with parameters registered as the first
  locals) + `Item::Import` forms into a fully self-consistent
  `Module` that round-trips through `Module::parse`.
- v0 opcode set + instruction codec (S5a / S5b.3 / S7 / S5d / S6.2 / S6.3a / S6.2b / S6.2c) — `Opcode`
  (43 frozen byte values; `0x82 host_call` added by S7; the integer
  bitwise/shift opcodes `band`/`bor`/`bxor`/`shl`/`shr` (0x36-0x3A) and
  `bnot` (0x51) added by S5d; the array opcodes
  `make_array`/`array_get`/`array_set`/`array_len` (0x60-0x63) added by
  S6.2; the tagged-aggregate opcodes `make_aggregate`/`get_field`/`get_tag`
  (0x64-0x66) added by S6.3a; the growable-array opcodes
  `array_push`/`array_pop` (0x67-0x68) added by S6.2b; the positional
  array opcodes `array_insert`/`array_remove` (0x69-0x6A) added by S6.2c,
  reusing `V0017`),
  `Imm { None, U32, I32, U32U32 }`,
  `Instruction` enum, `encode` / `decode` / `disassemble_text`.
  Defines the instruction stream inside `Function.code`. Stack
  manipulation (`Nop`, `Pop`), constants (`LoadConst U32`,
  `LoadTrue`, `LoadFalse`, `LoadNone`), locals (`LoadLocal U32`,
  `StoreLocal U32`), arithmetic (`Add`, `Sub`, `Mul`, `Div`,
  `Mod`, `Neg`), bitwise/shift on integers (`BitAnd`, `BitOr`,
  `BitXor`, `Shl`, `Shr`, `BitNot`), comparison
  (`Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`),
  logical (`Not`), arrays (`MakeArray U32`, `ArrayGet`, `ArraySet`,
  `ArrayLen`, `ArrayPush`, `ArrayPop`, `ArrayInsert`, `ArrayRemove`),
  tagged aggregates
  (`MakeAggregate U32U32`, `GetField U32`,
  `GetTag`), control flow (`Jump I32`, `JumpIfFalse I32`
  — both PC-relative to the byte after the immediate),
  function call (`Call U32U32` — `(fn_idx, argc)`), `Return` and
  host bridge (`HostCall U32U32` — `(import_idx, argc)`).
  Error code `B0012` `MALFORMED_INSTRUCTION` covers unknown
  opcodes and truncated immediates. S5c added the static
  verifier-error catalogue `B0013`-`B0020`
  (`VERIFIER_STACK_UNDERFLOW`, `VERIFIER_STACK_INCONSISTENCY`,
  `VERIFIER_FALL_OFF_END`, `VERIFIER_INVALID_RETURN_DEPTH`,
  `VERIFIER_LOCAL_OUT_OF_BOUNDS`, `VERIFIER_JUMP_OUT_OF_BOUNDS`,
  `VERIFIER_UNKNOWN_FUNCTION_INDEX`,
  `VERIFIER_CALL_ARITY_OVERFLOW`); `Vm::from_module` runs
  `verify_function` on every function and surfaces failures as
  `VmError::MalformedModule` with the verifier's stable code.
  S2.2b ships `match` + the pattern grammar in the frontend
  (parser + AST + dumper) and a second-cut emitter lowering for
  wildcard, ident-binding, literal, range (`..` exclusive,
  `..=` inclusive) and or-patterns (no new opcodes; reuses
  `StoreLocal`/`LoadLocal`/`LoadConst`/`Eq`/`Ge`/`Le`/`Lt`/
  `JumpIfFalse`/`Jump`/`LoadNone`). Identifier bindings inside
  or-pattern alts are restricted (binding-merging is type-aware
  and lands later). S6.3b adds enum support: an `enum` declaration
  registers each variant's wire-opaque tag (emitter-internal, never
  serialised), unit-variant construction (`Color::Red`, a `Path`)
  lowers to `MakeAggregate(tag, 0)`, tuple-variant construction
  (`Some(5)` / `Color::Pair(a, b)`, a `Call`) lowers to the args
  plus `MakeAggregate(tag, argc)`, and `match` `Path` /
  `TupleStruct` arms lower to a `GetTag` discriminant test followed
  by recursive `GetField` extraction of each payload sub-pattern.
  S6.3c adds `struct` support: a `struct` declaration registers a
  field layout (tag + declared field order), a struct-literal
  expression `Point { y: 2, x: 1 }` lowers to its initialisers
  reordered into declaration order plus `MakeAggregate(tag,
  field_count)`, and a `match` `Struct` pattern lowers to a `GetTag`
  test plus per-field `GetField` extraction resolved by field name.
  Struct literals are parsed only outside a "no-struct-literal" head
  context (`if` / `while` / `match` heads and `for` bounds), where
  `Path { ... }` is a path expression followed by a block. With
  S6.3a-c every `enum` / `struct` round-trips construct → `match`;
  `trait`/`impl` and extended type forms land in S2.3c.
  The
  `V<NNNN>` VM code prefix is now first occupied by S6.1; the
  `H<NNNN>` host ABI prefix remains
  reserved for S11.

Planned ABI surface (S5-S12):

- richer `match` patterns (tuple-struct / struct / path) — emitter
  lowering for these lands in a follow-up S2.2b sub-slice once enum
  data layout is defined;
- `trait`/`impl` items + extended type grammar (S2.3c);
- VM host call bridge (S7 / S8 / S9);
- host ABI modules: `time`, `log`, `fs` (sandboxed), `config`,
  `gfx2d`, `input`, `metrics`;
- stdlib subset accessible from bytecode;
- benchmark program contracts (`Snake`, `Asteroids` for Etapas 15-16).

CapyLang does **not** own:

- sandbox policy (CapyOS owns);
- process / resource limits at OS level (CapyOS owns);
- filesystem / network grants (CapyOS host adapter owns);
- UI / input / timer host functions (CapyOS adapter owns);
- staging / activation / rollback (CapyOS adapter owns);
- JIT (explicitly out of scope for the first integration wave).

## Compatibility rules

- Bytecode or IR formats must be explicitly versioned. The header
  is frozen by S4; body sections are versioned independently.
- Host ABI calls must be additive until the integration stage
  permits migration.
- Programs must not call CapyOS syscalls or use kernel pointers
  directly. All host resources are opaque handles.
- Filesystem, network, UI, input and timers must be accessed only
  through host ABI grants documented in the package descriptor
  `permissions`.
- JIT is out of scope for the first integration wave. The VM
  interprets bytecode only.
- The lexer (S1) is stable at v0.1.8; additive changes allowed
  within minor versions; major changes require version bump and
  migration note.

## Error model

| Code family | Trigger | VM behaviour |
|---|---|---|
| Lex error (S1) | `LexErrorKind::UnterminatedString` / `UnterminatedBlockComment` / `UnknownChar` | recoverable; lexer emits `Diagnostic` and continues producing `Token::Error` regions; never aborts |
| Parse error (S2.0) | malformed syntax | recoverable; parser emits `ParseDiagnostic` and inserts `Expr::Error` placeholder; never aborts, never panics |
| Bytecode load failure | header mismatch, checksum invalid, abi_version unsupported | VM refuses load with deterministic code |
| Host ABI call invalid | unknown handle, malformed argument | VM returns error to bytecode; never crashes host |
| Resource limit exceeded | instruction budget, time budget per frame, memory budget | VM aborts script; host receives deterministic error |
| Sandbox violation | bytecode attempts to call denied capability | VM rejects; host receives audit event |

All errors must be deterministic. CapyLang never returns
indeterminate state, never crashes the desktop on a script fault,
and never exposes raw pointers or syscall numbers.

## Resource and performance limits

| Limit | Default | Owner / configuration |
|---|---|---|
| Maximum lexer input size | configurable (alpha target: 1 MiB) | CapyLang |
| Maximum AST depth (S2) | TBD | CapyLang |
| Maximum bytecode container size | bounded by `payload_size` in manifest | CapyOS adapter |
| Bytecode header size | exactly 32 bytes (magic + bc_version + abi_version + flags + body_length + BLAKE3-128 checksum) | CapyLang ABI |
| VM instruction budget per frame | configurable; bounded by host adapter | CapyOS host ABI |
| VM time budget per frame | configurable; bounded by host adapter | CapyOS host ABI |
| Host resource handles | bounded by sandbox policy | CapyOS |
| Capy package payload | ≤ 1 MiB during the alpha streaming-buffer window | CapyOS adapter |

## Install/update boundary

CapyLang runtime artifacts may be optional Capy packages when Etapa
15 opens. CapyOS owns:

- sandbox policy enforcement;
- process / resource limits;
- filesystem / network grants;
- UI / input / timer host functions;
- staging, activation and rollback.

## Dependency rules

CapyLang components may depend on:

- `capy-lang-host` (the planned host ABI);
- `capy-benchmark-report` for benchmark workloads;
- CapyOS host ABI components explicitly listed by the package
  descriptor `permissions`.

They must not depend on kernel headers, raw filesystem, syscalls or
runtime internals.

## Validation before CapyOS integration

Before CapyOS consumes a CapyLang release, externally validate:

- lexer fixtures (23 goldens in `crates/capy-lexer/tests/fixtures/lexer/`);
- parser fixtures: 21 expression goldens in `crates/capy-parser/tests/fixtures/parser/` (S2.0 + S2.1 block + S2.2 control flow + S2.2b `match` / patterns) and 12 source-level goldens in `crates/capy-parser/tests/fixtures/source/` (S2.1 `let` + S2.2 `if`-stmt + S2.3a `fn`/`const`/`struct` + S2.3b `type`/`enum`/`import`); `trait`/`impl` and extended type forms land with S2.3c;
- bytecode/IR compatibility fixtures: `crates/capy-bytecode/tests/typed_round_trip.rs`, `crates/capy-bytecode/tests/instruction_pipeline.rs` and the verifier goldens in `crates/capy-bytecode/tests/verify_goldens.rs` (all 8 failure-mode codes `B0013`-`B0020` + principal success shapes incl. `HostCall`);
- VM deterministic execution: `crates/capy-vm/tests/end_to_end.rs` (source → emit → VM, including S7 `HostCall` dispatch through `HostAdapter::with_builtin_stubs()`); host-bridge handler contract pinned by `crates/capy-vm/src/host.rs` unit tests;
- host ABI mock tests (`capy-vm::HostAdapter` reference adapter, `time::now` / `log::info` stubs);
- sandbox / resource-limit behaviour;
- benchmark determinism when benchmark programs are included;
- `make rust-validate` (cargo fmt + clippy + workspace tests + doc tests);
- `make validate` (legacy doc/policy gates);
- `make package` produces canonical assets when the Etapa 15
  integration stage opens.

CapyLang integration is gated by Etapa 15.

## Publishing as a Capy package (Etapa 15, when the stage opens)

When CapyLang is delivered as a remote module to the CapyOS
`services/capypkg` adapter, the publisher must follow
[`CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`](../../CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md).
The key requirements that affect CapyLang are:

- `payload_url` must be HTTPS only;
- `payload_sha256` must be lowercase 64 hex of the published artifact;
- `payload_size` ≤ 1 MiB during the alpha streaming-buffer window;
- `name` must follow `[a-zA-Z0-9._-]`; suggested canonical name
  `org.capyos.lang.runtime`;
- `install_root` must live under `/var/capypkg` or `/opt/`;
- `signature_ed25519` must cover the canonical descriptor
  `name=N|version=V|payload_sha256=H|payload_url=U\n`;
- bytecode artefacts must declare their internal magic/version
  inside the payload itself; the adapter treats payload as opaque
  bytes.

Until CapyAgent publishes its Ed25519 signer, CapyLang cannot be
installed from a `signed` repository in production.

## Local development

CapyLang ships a `rust-toolchain.toml` pinning the `stable` channel.
After `rustup` is installed:

```bash
make build           # cargo build --workspace --all-targets
make test-rust       # cargo test  --workspace --all-targets + --doc
make fmt-check       # cargo fmt --all -- --check
make clippy          # cargo clippy --workspace --all-targets -- -D warnings
make rust-validate   # fmt-check + clippy + test-rust
make update-goldens  # CAPY_GOLDEN_UPDATE=1 cargo test (rewrites .tokens)
make validate        # legacy doc/policy gates (no Rust required)
```

The `capyc-tokens` debug CLI exits `0` on a clean lex, `1` when at
least one diagnostic was emitted and `2` on usage / I/O errors.

## Continuous integration

`.github/workflows/ci.yml` runs `make validate`;
`.github/workflows/rust.yml` runs the Rust pipeline (fmt, clippy,
target tests, doctests, anti-drift on goldens).

## Integration rule

CapyOS may only load CapyLang artifacts through a versioned host ABI
and sandboxed bytecode loader. This repository must remain
buildable and testable without CapyOS kernel headers.
