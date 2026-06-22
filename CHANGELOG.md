# Changelog

All notable changes to CapyLang are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Slice identifiers (e.g. **S1**, **S2**) refer to the roadmap captured in
`README.md` and trace each release back to a CapyOS-Etapa-15 integration gate.

## [Unreleased]

## [0.1.11] - 2026-06-21

### Added
- S10: integer `clamp(x, lo, hi)` numeric builtin = `max(lo, min(x, hi))`.
  Emitter-only lowering onto two compare + conditional-jump selects (no new
  opcode, no const-pool), composing the `min`/`max` lowering; a user-defined
  `fn clamp` still shadows the builtin, and wrong arity reports `E0022`.

## [0.1.10] - 2026-06-18

### Added
- S10: integer `min(a, b)` / `max(a, b)` numeric builtins. Emitter-only
  lowering onto the existing compare + conditional-jump opcodes (no new wire,
  no new const-pool use); a user-defined `min` / `max` still shadows the
  builtin. Wrong arity is reported as `E0022` (message generalized from
  array-method-specific to also cover numeric builtins).

## [0.1.9] - 2026-06-17

### Added

- **S10 (first slice) — array methods at source level (`a.push(x)`,
  `a.pop()`, `a.insert(i, x)`, `a.remove(i)`, `a.get(i)`, `a.set(i, x)`,
  `a.len()`).** Makes the growable-array opcodes usable from CapyLang source
  for the first time. The emitter lowers a method-call whose callee is a field
  access (`recv.method(args)`) onto the matching array opcode, emitting the
  receiver then the arguments in source order (so `a.set(i, v)` →
  `arr idx val` + `ArraySet`). **Emitter-only — no new opcodes or wire
  change**: it reuses the existing `0x60-0x6A` array opcodes, and the parser
  already produced `Call { callee: Field { .. }, .. }` for the postfix
  `.name(...)` form, so only `emit_call` grew an array-method arm. An unknown
  method is fail-closed (`E0012 UNSUPPORTED_FEATURE`); a wrong argument count
  for a known method is the new `E0022 METHOD_ARITY`. Five end-to-end tests
  cover `push`/`len`/`insert` lowering plus the two fail-closed paths. This
  closes the "the `a.push(x)` / `a.pop()` frontend surface waits on S10" note
  the S6.2b/S6.2c slices left open (array reads/writes via `a[i]` / `a[i] = v`
  index syntax were already lowered by S6.2).
- **S6.2c — array insert / remove (`array_insert` / `array_remove`,
  bytecode + VM).** Completes the growable-array surface S6.2b started by
  adding positional insert / remove, so a bound array can grow or shrink at
  an arbitrary index instead of only at the tail (the snake body can splice,
  not just append). Two additive opcodes in the reserved `0x60-0x6F`
  aggregate block: `array_insert` (0x69, no immediate) pops `(arr, idx, val)`,
  inserts `val` at `idx` in the shared backing **in place** (shifting `idx..`
  right, growing by one) and pushes the same array handle back (reference
  semantics, mirroring `array_set`); `array_remove` (0x6A, no immediate) pops
  `(arr, idx)`, removes and pushes the element at `idx` (shifting `idx+1..`
  left, shrinking by one). An `idx == len` insert appends (matching
  `array_push`); a negative index, `idx > len` (insert) or `idx >= len`
  (remove, including any index into an empty array) is fail-closed with the
  existing `V0017 INDEX_OUT_OF_BOUNDS` — **no new trap** and no wire change
  beyond the two opcode bytes. Bytecode + VM only (like S6.2b); the
  `a.insert(i, x)` / `a.remove(i)` frontend surface waits on the method-call
  / stdlib work (S10).
  - **Surface**: `Opcode::{ArrayInsert,ArrayRemove}` (0x69 / 0x6A) and the
    matching no-immediate `Instruction` variants; the verifier stack effects
    (`ArrayInsert` requires 3 / produces 1, `ArrayRemove` requires 2 /
    produces 1); the VM `array_insert` / `array_remove` helpers reusing the
    `expect_array` / `expect_index` primitives and the `IndexOutOfBounds`
    trap. `docs/bytecode-v0.md`, `docs/compatibility.md`, `docs/aggregates.md`,
    `docs/roadmap.md` and `README.md` updated (frozen opcode count 41 → 43;
    no error-catalogue change — `V0017` reused).
  - **Tests**: opcode round-trip + uniqueness arrays extended; the
    instruction codec round-trip covers both new variants; three verifier
    stack-effect tests (balanced insert-then-remove, `array_insert`
    underflow, `array_remove` underflow); seven VM tests (insert places +
    shifts, insert at `len` appends like push, insert past `len` traps
    `V0017`, remove returns the removed element, remove shifts survivors
    through an alias proving reference semantics, remove out-of-range traps
    `V0017`, and a `V0005` insert-on-non-array trap).
  - **Decoupling preserved**: additive opcode growth within v0; the 32-byte
    header, section tags and existing opcodes are untouched, and determinism
    / fail-closed contracts hold.
- **S6.2b — growable arrays (`array_push` / `array_pop`, bytecode + VM).**
  Extends the S6.2 array machinery so a bound array can change length,
  closing the gap between "fixed-capacity array" and the variable-length
  data (the snake body) S6.2 was meant to unblock. Two additive opcodes in
  the reserved `0x60-0x6F` aggregate block: `array_push` (0x67, no
  immediate) pops `(arr, val)`, appends `val` to the shared backing **in
  place** and pushes the same array handle back (reference semantics,
  mirroring `array_set`); `array_pop` (0x68, no immediate) pops `arr` and
  pushes its removed last element. A push / pop on a non-array still traps
  with `V0005 TYPE_MISMATCH`; the new `V0019 POP_EMPTY_ARRAY` trap covers a
  fail-closed pop of an empty array (kept distinct from the index-oriented
  `V0017` so the message stays precise). This sub-slice is **bytecode + VM
  only** — testable by building modules directly, with no frontend changes;
  the `a.push(x)` / `a.pop()` surface waits on the method-call / stdlib
  work (S10).
  - **Surface**: `Opcode::{ArrayPush,ArrayPop}` (0x67 / 0x68) and the
    matching no-immediate `Instruction` variants; the verifier stack
    effects (`ArrayPush` requires 2 / produces 1, `ArrayPop` requires 1 /
    produces 1); `VmError::PopEmptyArray` + the `V_POP_EMPTY_ARRAY`
    constant (re-exported from the `capy-vm` crate root); the VM
    `array_push` / `array_pop` helpers. `docs/bytecode-v0.md`,
    `docs/compatibility.md`, `docs/aggregates.md`, `docs/roadmap.md` and
    `README.md` updated (frozen opcode count 39 → 41; error catalogue
    `V0001`-`V0018` → `V0019`).
  - **Tests**: opcode round-trip + uniqueness arrays extended; the
    instruction codec round-trip covers both new variants; three verifier
    stack-effect tests (balanced push-then-pop, `array_push` underflow,
    `array_pop` underflow); five VM tests (push grows + `array_len`
    reports the new size, `array_pop` returns/removes the last element,
    push visible through an alias proving reference semantics, the `V0019`
    empty-pop trap, and a `V0005` push-on-non-array trap); the VM error
    catalogue's every-variant display test extended.
  - **Decoupling preserved**: additive opcode growth within v0; the
    32-byte header, section tags and existing opcodes are untouched, and
    determinism / fail-closed contracts hold.

## [0.1.8] - 2026-06-02

### Added

- **S6.3c — struct construction + `match` struct patterns (frontend +
  emitter).** Completes S6.3 so `enum` and `struct` values both
  round-trip construct → destructure, with **no new opcodes or wire
  change**. New AST node `Expr::StructLit { path, fields, span }` and its
  `StructLitField` companion. The parser gains a `no_struct_literal`
  context: `Path { ... }` is a struct literal everywhere **except** the
  head of `if` / `while` / `match` and the bounds of `for` (where the `{`
  opens a block); the suppression is reset inside every delimiter (parens,
  call args, array elements, index, block bodies, struct-literal field
  values and `match` arms), so `if f(P { x: 1 }) { .. }` still parses the
  inner literal. Pass 0 registers each `struct`'s tag and declared field
  order; a struct literal lowers its initialisers **reordered into
  declaration order** then `MakeAggregate(tag, field_count)` (so
  `Point { y: 2, x: 1 }` stores `x` at field 0); a `match` `Struct`
  pattern lowers to a `GetTag` test plus per-field `GetField` resolved by
  field name (shorthand `Point { x }` binds `x`; `..` ignores unlisted
  fields). `struct` items now emit no diagnostic (previously `E0002`).
  Field-access `p.x` stays deferred to the type checker.
  - **Surface**: `capy_ast::Expr::StructLit` + `StructLitField` (with
    `dump_expr` `StructLit` / `LitField` rendering); `capy_parser`
    `no_struct_literal` field + `parse_head_expr` / `parse_delimited_expr`
    / `parse_struct_literal_body`; `capy_emitter` struct registry
    (`StructLayout`), `emit_struct_lit`, `emit_tag_eq_test`,
    `struct_field_index` and the `Pattern::Struct` lowering. No
    `capy-bytecode` / `capy-vm` change.
  - **Tests**: 8 parser unit tests (`struct_lit_tests`: literal / shorthand
    / qualified path parsing + `if` / `while` / `match`-head regression
    guards + struct-literal-inside-block / -call-args); 1 emitter
    instruction test (field reordering); 6 VM end-to-end tests
    (construct + destructure, field reordering, literal field sub-pattern
    with fall-through, aggregate-valued construction, struct inside an
    `if` body, struct + enum sharing the tag space).
  - **Validation note**: the struct-literal parser change is the
    highest-risk part — the 23 lexer + 33 parser golden fixtures and the
    control-flow source fixtures must be re-run externally to confirm no
    regression in existing parses.
  - **Decoupling preserved**: additive AST / parser / emitter growth
    within v0; the 32-byte header, opcode set and section tags untouched.

- **S6.3b — enum construction + `match` variant lowering (emitter).**
  Builds the frontend half of S6.3 on top of the S6.3a value model, with
  **no new opcodes or wire change**. A pass-0 registry over `Item::Enum`
  assigns every variant a wire-opaque, emitter-internal `tag` (never
  serialised). Construction lowers syntactically: a unit variant written
  as a path (`Color::Red`) becomes `MakeAggregate(tag, 0)`; a tuple
  variant call (`Some(5)`, or `Color::Pair(a, b)`) emits its arguments
  then `MakeAggregate(tag, argc)` — `emit_call` consults the registry
  before the `fn` / `import` resolution so a variant callee wins. `match`
  gains two pattern arms: `Pattern::Path` (unit variant) lowers to a
  `LoadLocal scrut; GetTag; LoadConst Int(tag); Eq; JumpIfFalse next`
  discriminant test, and `Pattern::TupleStruct` (`Some(x)`, `Pair(1, b)`)
  adds, after the tag test, a per-field `GetField` extraction into a
  fresh local that the sub-pattern recurses on (literal sub-patterns
  test the field, identifier sub-patterns bind it). `enum` declarations
  now emit no diagnostic (previously `E0002`).
  - **Surface**: `capy_emitter` only — `ModuleEmitter` gains a
    `variant_registry`; `FunctionEmitter` gains the `extract_field` /
    `emit_variant_tag_test` helpers and lowers `Expr::Path` /
    `Expr::Call` variant construction plus the `Path` / `TupleStruct`
    `match` arms. No `capy-bytecode` / `capy-vm` change. v0 resolution
    is by variant name (path last segment); unit variants are written
    qualified until a name resolver lands (see `docs/structs-enums.md`).
    `struct` literals and `Struct` patterns stay `E0010` (S6.3c).
  - **Tests**: 4 emitter instruction-level tests (unit + tuple
    construction shapes, enum-emits-no-code, `match` uses `GetTag` /
    `GetField`); 7 VM end-to-end tests (unit-variant dispatch, bound
    unit variant, tuple payload binding, two-field binding, literal
    field sub-pattern with fall-through, aggregate-valued construction,
    wildcard fall-through).
  - **Decoupling preserved**: purely additive emitter growth within v0;
    the 32-byte header, opcode set and section tags are untouched.

- **S6.3a — tagged-aggregate value model (struct / enum at runtime,
  bytecode + VM).** The second composite runtime value, building directly
  on the S6.2 array machinery. `Value::Aggregate { tag, fields }` is a
  reference-semantics `Rc<RefCell<Vec<Value>>>` handle (aliases share one
  backing store; opaque to bytecode) representing a struct instance or an
  enum variant; `tag` is an emitter-assigned, wire-opaque discriminant the
  VM only stores and compares. Three additive opcodes: `make_aggregate`
  (0x64, `U32U32` immediate `(tag, field_count)`), `get_field` (0x65,
  `U32` field index) and `get_tag` (0x66). The new
  `V0018 FIELD_OUT_OF_BOUNDS` trap covers an out-of-range `get_field`
  index (a `get_field` / `get_tag` on a non-aggregate still traps with
  `V0005 TYPE_MISMATCH`). This sub-slice is **bytecode + VM only** —
  testable by building modules directly, with no frontend changes; enum
  construction, struct literals and the `match` tuple-struct / struct /
  path lowering follow in S6.3b / S6.3c and add no new opcodes.
  - **Surface**: `Opcode::{MakeAggregate,GetField,GetTag}` and the
    matching `Instruction` variants (`MakeAggregate { tag, field_count }`,
    `GetField(u32)`, `GetTag`); `Value::Aggregate`; `VmError::FieldOutOfBounds`
    + the `V_FIELD_OUT_OF_BOUNDS` constant; `docs/bytecode-v0.md`,
    `docs/compatibility.md` and `docs/structs-enums.md` updated; full
    design in `docs/structs-enums.md`.
  - **Tests**: opcode round-trip + uniqueness arrays extended; instruction
    codec round-trip, a truncated-`U32U32`-immediate rejection and a
    disassembly format test; verifier stack-effect tests (`make_aggregate`
    underflow + a balanced build-and-read program); VM end-to-end tests for
    `make_aggregate`/`get_tag`, `get_field`, the `V0018` and
    `V0005` traps, and an aggregate that composes with locals + arithmetic
    (`#{0}(3, 4)` → `p.x + p.y` → `7`); the VM error catalogue's
    every-variant display test extended.
  - **Review fix**: `V_INDEX_OUT_OF_BOUNDS` (V0017, added by S6.2) was
    never re-exported from the `capy-vm` crate root; both it and the new
    `V_FIELD_OUT_OF_BOUNDS` are now public.
  - **Decoupling preserved**: additive opcode growth within v0; the
    32-byte header is untouched and no body section changed.

## [0.1.7] - 2026-05-29

### Added

- **S6.2 — aggregate value model: arrays.** The first composite runtime
  value. `Value::Array` is a reference-semantics `Rc<RefCell<Vec<Value>>>`
  so a bound array is mutated in place and aliases share one backing
  store; the handle is opaque to bytecode. Four additive opcodes:
  `make_array` (0x60), `array_get` (0x61), `array_set` (0x62) and
  `array_len` (0x63); the new `V0017 INDEX_OUT_OF_BOUNDS` trap covers a
  negative or out-of-range index (a non-array / non-int operand still
  traps with `V0005 TYPE_MISMATCH`). The frontend gains `Expr::Array`,
  array literals `[e0, ...]`, indexing `a[i]` (lowered to `array_get`,
  replacing the former `E0001` rejection) and indexed assignment
  `a[i] = v` (the first non-identifier assignment target). Arrays trap in
  the arithmetic / comparison opcodes via the existing fall-through arms;
  an in-language structural `==` is deferred (the derived `PartialEq` is
  element-wise for host-side tests only).
  - **Surface**: `Opcode::{MakeArray,ArrayGet,ArraySet,ArrayLen}` and the
    matching `Instruction` variants; `docs/bytecode-v0.md`,
    `docs/compatibility.md` and `docs/grammar.ebnf` updated; full design
    in `docs/aggregates.md`.
  - **Tests**: opcode + instruction round-trip arrays extended; VM
    end-to-end tests for literal / index, in-place element assignment,
    out-of-bounds trap, and a `for` loop that fills then sums an array
    (proving S2.4 + S2.5 + S6.2 compose).
  - **Decoupling preserved**: additive opcode growth within v0; the
    32-byte header is untouched.

- **S2.4 — assignment and mutation.** New `Expr::Assign { target, value }`
  AST node; the parser recognises `=` and the compound operators `+=`,
  `-=`, `*=`, `/=`, `%=` (desugared to `target = target <op> value`) at
  the lowest precedence, right-associative. The emitter lowers an
  assignment to a local into `<value>` + `StoreLocal` + `LoadNone` (the
  expression evaluates to unit), so reassignable locals make imperative
  `while` / `loop` counters expressible end-to-end. Only a simple local
  identifier is assignable in v0; other targets raise the new typed
  `E0021 INVALID_ASSIGN_TARGET`, and assigning to an undeclared name
  reuses `E0003 UNKNOWN_LOCAL`. No new opcodes — reuses `StoreLocal` /
  `LoadNone`; the v0 instruction set frozen by S5a is unchanged.
  - **Surface**: `capy_ast::Expr::Assign`; emitter
    `EmitErrorKind::InvalidAssignTarget` (`E0021`); new `assignment` /
    `assign_op` productions in `docs/grammar.ebnf`.
  - **Tests**: 5 parser unit tests (`crates/capy-parser/src/parser.rs`,
    module `assign_tests`); 3 emitter tests (exact lowering plus the two
    rejection codes) in `crates/capy-emitter/tests/end_to_end.rs`; 3 VM
    end-to-end tests in `crates/capy-vm/tests/end_to_end.rs` (plain
    assignment, compound assignment, and a mutating `while` loop that
    sums `1..=5` to `15`).
  - **Decoupling preserved**: no wire-format change; purely additive AST
    + emitter growth within v0.

- **S2.5 — `for` loops over integer ranges.** New `Expr::For { var,
  start, end, inclusive, body }` AST node and `for <i> in <a>..<b> { … }`
  / `..=` parsing (the inclusive form uses the same `..`-then-adjacent-`=`
  rule as range patterns — there is no `..=` lexer token). The emitter
  lowers it to an initialise / poll / body / increment loop reusing
  existing opcodes (`StoreLocal`, `LoadLocal`, `Add`, `Lt` / `Le`,
  `JumpIfFalse`, `Jump`); `continue` targets the increment so the loop
  variable always advances (unlike a naive parser desugaring). No new
  opcode or wire change. v0 caveats: the `end` bound is re-evaluated each
  iteration and the loop variable is a function-scoped local.
  - **Tests**: parser test `for_loop_parses_exclusive_and_inclusive`;
    3 VM end-to-end tests (`for_loop_inclusive_range_sums` = 15,
    `for_loop_exclusive_range_sums` = 10,
    `for_loop_with_continue_still_advances` = 5).

- **S5d — integer bitwise & shift operators.** The operators `&`, `|`,
  `^`, `<<`, `>>` and unary `~` (parsed since S2.0 but previously
  rejected by the emitter) now lower and execute. Six opcodes are added
  to the v0 set (additive): `band` (0x36), `bor` (0x37), `bxor` (0x38),
  `shl` (0x39), `shr` (0x3A) and `bnot` (0x51). The emitter maps the
  matching `BinOp` / `UnOp` variants directly (replacing the former
  `E0008` / `E0009` rejections, which stay reserved with no producer);
  the VM executes them on `Int` operands only — a non-integer operand
  traps with `V0005 TYPE_MISMATCH`, and shift counts are reduced modulo
  64 via `wrapping_shl` / `wrapping_shr`, so the VM stays total and
  deterministic. The static verifier treats the bitwise binaries like
  `Add` (pop 2, push 1) and `bnot` like `Neg` (pop 1, push 1).
  - **Surface**: `Opcode::{BitAnd,BitOr,BitXor,Shl,Shr,BitNot}` plus the
    matching `Instruction` variants; the `docs/bytecode-v0.md`
    instruction table is extended.
  - **Tests**: opcode and instruction round-trip arrays extended; 3 VM
    end-to-end tests (`bitwise_operators_evaluate`,
    `bitwise_not_complements_int`,
    `bitwise_on_non_int_traps_type_mismatch`) and 2 emitter lowering
    tests in `crates/capy-emitter/tests/end_to_end.rs`.
  - **Decoupling preserved**: additive opcode growth within v0; no
    header or section-format change.

- **S12 — `capyc check` subcommand.** New `capyc check [PATH]` runs the
  lexer, parser and (when parsing is clean) the emitter, then renders
  every diagnostic through `capy-diagnostics::render` — the rustc-style
  block with `--> file:line:col`, the source line and a `^^^` caret —
  instead of the terse inline format the other subcommands use. Lexer
  diagnostics arrive via the parser's `ParseErrorKind::Lex` entries, so
  `bridge::from_parse` covers the `L*` and `P*` families without a
  duplicate lexer pass; emitter failures use `bridge::from_emit` (`E*`).
  This is the first consumer of the otherwise-unused `capy-diagnostics`
  dependency declared by `capyc`. Exit codes follow the existing
  contract (`0` clean, `1` diagnostics, `2` usage / I/O).

- **S12 — `capyc repl` subcommand.** New `capyc repl` reads one line at a
  time from stdin, wraps each as the body of an implicit `fn main() { … }`,
  compiles and executes it, and prints the resulting value. The prompt and
  diagnostics go to stderr and successful values to stdout, so it composes
  with pipes (`echo '1 + 2' | capyc repl`). State does not persist across
  lines in this v0 sketch. With `check` and `repl` landed, the S12 command
  set (`tokens` / `parse` / `check` / `compile` / `disasm` / `run` /
  `repl`) is complete.

- **String concatenation via `+`.** The `Add` opcode now concatenates two
  `Str` operands (`"foo" + "bar"` → `"foobar"`) in addition to numeric
  addition; `Sub` / `Mul` / `Div` / `Mod` stay numeric-only. No new
  opcode or wire-format change — the emitter already lowered `+` to
  `Add`; only the VM's runtime semantics gained the `(Str, Str)` case.
  Mixed operands (e.g. `1 + "x"`) still trap with `V0005 TYPE_MISMATCH`.

### Changed

- **Release hygiene / version reconciliation.** The Rust workspace
  version was lagging at `0.1.3` while `VERSION`, `README.md` and the
  `v0.1.4`-`v0.1.6` tags already advertised `0.1.6`. Bumped `Cargo.toml`
  `workspace.package.version` and the nine workspace-crate entries in
  `Cargo.lock` to `0.1.6`, and consolidated the accumulated S2-S7
  engineering notes (previously parked under `[Unreleased]`) into a dated
  `[0.1.6]` section.
- **Documentation drift fixes.** `docs/bytecode-v0.md` now documents the
  static-verifier error catalogue `B0013`-`B0020` and drops the stale
  "to be populated during S4" / "open questions" framing; `README.md`
  corrects the `capy-vm` error range to `V0001..V0016`.
- **`.windsurf` guidance refresh.** The `capylang-project-map` and
  `capylang-abi-contract` skills, the `00-project-authority` and
  `20-abi-compatibility` rules, `.windsurf/README.md` and the workflow
  pin/version references now reflect the delivered S1-S7 surface and the
  pinned CapyOS core `0.8.0-alpha.260+20260525`.
- **Master development plan.** Added `docs/roadmap.md` — an authoritative,
  ordered roadmap with per-slice dependencies, acceptance criteria and the
  validation gate, sequencing the remaining work (aggregate value model ->
  `struct`/`enum` + richer `match` -> host ABI -> Etapa 15 -> benchmarks)
  toward the Snake / Asteroids goal. Repaired the dangling "authoritative
  roadmap reference" in `docs/bytecode-v0.md` (it pointed at a
  non-existent `.windsurf/plans/...` path) to point at it, and registered
  it in the authority order (README documentation map, the
  `capylang-project-map` skill and the `00-project-authority` rule).
- **S6.3 design.** Added `docs/structs-enums.md`, the implementation-ready
  design for `struct` / `enum` at runtime and the `match` tuple-struct /
  struct / path lowering, building on the S6.2 aggregate machinery: a new
  `Value::Aggregate`, opcodes `make_aggregate` / `get_field` / `get_tag`
  (0x64-0x66), `V0018 FIELD_OUT_OF_BOUNDS`, a compile-time tag / layout
  registry that needs no type checker, the struct-literal parser
  ambiguity, and an S6.3a/b/c staging. (`docs/aggregates.md`, the S6.2
  design, is now implemented — see Added.) Design only.

## [0.1.6] - 2026-05-21

Consolidated feature release. The `0.1.4` and `0.1.5` tags were
packaging / CI-formatting releases without a dedicated changelog entry;
the engineering notes for the whole **S2-S7** arc are gathered here under
the `0.1.6` tag that the working tree currently carries. That arc covers
the recursive-descent parser + span-preserving AST (S2.0-S2.3b, plus the
S2.2b `match`/pattern frontend), structured diagnostics (S3), the
bytecode v0 container and typed sections (S4/S4b), the v0 opcode set and
the static stack-balance verifier (S5a/S5c), the AST -> bytecode emitter
(S5b.1-S5b.3 + S7 import lowering), the deterministic VM core (S6.1) and
the host-call bridge (S7), together with the `capyc` unified CLI sketch
(S12). The lexer (S1) is unchanged and remains additive within v0.

### Added

- **Debug section wiring** — emitter ships `DebugInfo`; bridge
  resolves VM `pc` → source span.
  - `FunctionEmitter` gains a `debug_span_stack` maintained by
    `emit_expr`, plus a `debug_entries` list populated by a new
    `record_debug()` hook called from `emit_op`, `emit_jump` and
    each `Instruction::Call` / `HostCall` site. Entries map the
    code byte offset of each opcode to the innermost `Expr`'s
    source span; nested `emit_expr` calls at the same offset
    prefer the narrower span.
  - `ModuleEmitter` aggregates per-function debug entries into a
    new `debug_per_fn: Vec<Vec<DebugEntry>>` and emits a `Debug`
    section in the module **for function 0 only** when at least
    one entry was recorded. (The v0 wire format lacks a function-
    index field on `DebugEntry`; other functions' entries stay
    in `debug_per_fn` for future inspection, awaiting v1.)
  - `VmError` gains a `pc(&self) -> Option<u32>` accessor: every
    runtime-pc-tied variant returns `Some(pc)`; `BudgetExhausted`,
    `UnknownFunction` and `MalformedModule` return `None`.
  - `capy-diagnostics` adds `bridge::from_vm_with_debug(&VmError,
    &DebugInfo) -> Diagnostic`. The lookup uses nearest-not-greater
    semantics over the entry list; pc-less variants and missing
    debug entries gracefully fall back to `Span::new(0, 0)`.
  - The bridge is forward-compatible: a future signature
    `from_vm_with_debug_v1(&VmError, &DebugInfo, function_index)`
    can be added once v1 adds the function-index field.
  - **Tests**: 3 new emitter end-to-end tests
    (`module_emits_debug_section_with_source_spans`,
    `module_omits_debug_section_for_an_empty_source`, and a
    structural check that bytecode_offsets are non-decreasing);
    6 new bridge tests in `crates/capy-diagnostics/src/bridge.rs`
    (`vm_with_debug_resolves_pc_to_nearest_not_greater_entry`,
    `vm_with_debug_exact_offset_match_picks_that_entry`,
    `vm_with_debug_pc_before_first_entry_falls_back_to_origin`,
    `vm_with_debug_pcless_variants_fall_back_to_origin`,
    `vm_with_debug_empty_debug_info_is_safe`,
    `vm_error_pc_accessor_round_trip`); plus 3 cross-crate
    integration tests in
    `crates/capy-diagnostics/tests/end_to_end_debug.rs`
    (`division_by_zero_trap_resolves_to_div_span`,
    `rendered_runtime_diagnostic_contains_source_caret`,
    `pcless_error_falls_back_to_origin_span_even_with_debug`).
  - **Decoupling preserved**: no new opcodes, no wire-format
    change. The optional `Debug` section was already part of the
    v0 spec (`SectionTag::Debug = 0x04`); this slice only starts
    populating it.

- **S3 follow-up** — `capy-diagnostics` bridges for emitter + VM.
  - New `bridge::from_emit(&EmitError) -> Diagnostic` lifts an
    `EmitError` into the unified `Diagnostic` shape: stable
    `E<NNNN>` code, prose from a new `EmitError::message()` method
    (no `[code]` prefix; the renderer prints the code in the
    header), primary span taken from `EmitError::span`.
  - New `bridge::from_vm(&VmError) -> Diagnostic` lifts a `VmError`
    into a `Diagnostic`. Because the VM addresses errors by `pc`
    rather than source byte offset, the bridge uses an empty
    primary `Span::new(0, 0)` and preserves the message tail
    (which already carries `pc`, `module::symbol`, host reasons,
    inner `B<NNNN>` codes, etc.). Downstream tooling that owns a
    `DebugInfo` lookup table may resolve `pc` → source span and
    replace `primary.span` before rendering.
  - `EmitError` gained `Display + std::error::Error` impls and a
    new `EmitError::message()` helper that returns the prose body
    without the `[E<NNNN>]` prefix. The Display impl now reads
    `[E<NNNN>] <message>` so the CLI, the bridge and any log line
    still pin the diagnostic to a stable code.
  - `capyc` CLI: the `render_emit_error` helper now formats via
    `Display` (`emitter [a..b] [E<NNNN>] <message>`) instead of
    Debug.
  - **Dependencies**: `capy-diagnostics` gains workspace deps on
    `capy-bytecode`, `capy-emitter` and `capy-vm`. No cycle is
    introduced — those crates do not depend back on
    `capy-diagnostics`.
  - **Public surface**: `capy_diagnostics::from_emit` and
    `capy_diagnostics::from_vm` are re-exported from the crate
    root next to the existing `from_lex` and `from_parse`.
  - **Tests**: 7 new bridge tests in
    `crates/capy-diagnostics/src/bridge.rs`
    (`emit_unknown_function_maps_to_e0016_with_span`,
    `emit_duplicate_import_threads_name`,
    `emit_message_strips_code_prefix_from_display`,
    `vm_division_by_zero_maps_to_v0006`,
    `vm_default_span_is_empty_at_origin`,
    `vm_malformed_module_threads_inner_bytecode_code_into_message`,
    `vm_host_call_failed_preserves_reason_verbatim`). The existing
    `from_lex` / `from_parse` tests are unchanged.

- **Diagnostics polish** — `Display` impls for `VmError` and
  `BytecodeError`.
  - Both error types now implement `std::fmt::Display` and
    `std::error::Error`. Output is deterministic, includes the
    stable `V<NNNN>` / `B<NNNN>` code in square brackets, and
    renders `pc` / `offset` fields in zero-padded hex
    (`pc=0x0010`). The format is **stable within v0** —
    downstream snapshot tests should match the code prefix, not
    the prose tail.
  - Privacy: every field surfaced by the new Display arms came
    over the v0 ABI (a `&'static str`, a bounded numeric index,
    or a program-declared symbol). No host paths, environment
    variables or wall-clock values cross the boundary;
    `HostCallFailed` carries only the handler's static
    `reason`. The Display layer therefore inherits the
    `docs/integration.md` "host call privacy" guarantee from
    the underlying ABI.
  - The `capyc` CLI now formats VM and bytecode load errors
    through Display (`format!("runtime: {e}")`,
    `format!("load bytecode: {e}")`). The hand-rolled
    `error_blurb` lookup table and the `vm_error_code` /
    `vm_error_blurb` shim helpers are removed — Display owns
    the prose, `code()` owns the routing key.
  - Tests: 5 new VmError Display tests in
    `crates/capy-vm/src/error.rs` (`display_includes_stable_code_and_pc`,
    `display_division_by_zero_is_redacted`,
    `display_host_call_failed_includes_reason_verbatim`,
    `display_malformed_module_threads_inner_bytecode_code`,
    `display_is_deterministic_under_repeat`,
    `every_variant_displays_with_its_code_prefix`) plus 3
    BytecodeError Display tests in
    `crates/capy-bytecode/src/error.rs`
    (`display_includes_stable_code_prefix`,
    `display_renders_offset_in_hex`,
    `every_variant_displays_with_its_code`).
  - This sets the table for the planned `capy-diagnostics`
    bridge: `bridge::from_vm` / `bridge::from_emit` can wrap
    Display directly without re-encoding error payloads.

- **S2.2b (emitter lowering, second cut)** — or-patterns and range
  patterns now lower into v0 bytecode using existing opcodes.
  - **Range patterns** (`lo..hi`, `lo..=hi`) lower to two bounds
    checks: `LoadLocal scrut; LoadConst lo; Ge; JumpIfFalse next_arm`
    followed by `LoadLocal scrut; LoadConst hi; (Lt | Le);
    JumpIfFalse next_arm`. The `Lt`/`Le` selection follows the
    `inclusive` flag from the AST: `..=` uses `Le`, `..` uses `Lt`.
    Range endpoints must be `Pattern::Literal`; non-literal
    endpoints surface a typed
    `EmitErrorKind::UnsupportedFeature { what: "non-literal range
    endpoint" }` so a future grammar relaxation cannot silently
    emit wrong code.
  - **Or-patterns** (`alt0 | alt1 | … | altN-1`) lower to a chain
    where each non-last alt's success jumps to a shared
    `body_label`; the last alt's failure target is the outer
    arm's `next_arm`. Successful alts converge at `body_label` so
    the body is emitted once. Each alt is lowered through
    `emit_pattern_test`, so or-patterns transparently support
    `Literal`, `Range` and `Wildcard` alternatives.
  - **Restriction**: identifier bindings inside or-pattern alts
    (`1 | x`) surface
    `EmitErrorKind::UnsupportedFeature { what: "identifier binding
    in or-pattern" }`. Binding-merging across alts requires
    type-aware machinery that the v0 emitter does not own yet; it
    lands alongside a later type-checker slice.
  - **Verifier**: no changes. The new lowering reuses the existing
    `Ge`/`Le`/`Lt`/`Eq`/`JumpIfFalse`/`Jump` opcodes. Stack
    discipline is preserved at every join (each pattern test
    leaves the stack at the pre-test depth; `body_label` is
    reached with the same depth from every alt's success path).
  - **Tests**: 4 new emitter end-to-end tests in
    `crates/capy-emitter/tests/end_to_end.rs`
    (`match_range_pattern_lowers_to_bounds_checks`,
    `match_exclusive_range_uses_lt_for_upper_bound`,
    `match_or_pattern_chains_alts_with_shared_body`,
    `match_or_pattern_rejects_ident_alt`) plus 5 new VM
    end-to-end tests in `crates/capy-vm/tests/end_to_end.rs`
    (`match_inclusive_range_pattern_matches_inside_bounds`,
    `match_exclusive_range_pattern_excludes_upper_bound`,
    `match_or_pattern_matches_any_alternative`,
    `match_or_pattern_combines_with_guard`,
    `match_range_pattern_combines_with_wildcard_fallback`). The
    pre-existing `match_unsupported_pattern_emits_typed_error`
    test was updated to target struct patterns (still
    unsupported), since `Range` and `Or` are now positive cases.
  - **README**: `S2.2b match + patterns` status flips to "second
    cut done (literal/wildcard/ident/range/or; tuple-struct/
    struct/path deferred)".

- **S2.2b (emitter lowering, first cut)** — `match` expressions now
  lower into v0 bytecode using existing opcodes only; no new wire
  surface.
  - **Lowering shape** (in `crates/capy-emitter/src/emit.rs`): the
    scrutinee is evaluated once and stored into a fresh synthetic
    local (allocated via the new internal `alloc_unnamed_local`);
    each arm emits a pattern test that pushes a `Bool` and branches
    on `JumpIfFalse` to the next arm, optionally evaluates the
    guard with the same shape, then evaluates the body and jumps to
    a shared `end` label. A trailing `LoadNone` covers
    non-exhaustive fall-through so the verifier sees a stack-
    discipline-clean +1 net effect on every reachable path.
  - **Supported patterns in this first cut**: `_` (wildcard), an
    identifier binding (`Pattern::Ident`, which always matches and
    binds a fresh local), and `Pattern::Literal` (compared via the
    existing `Eq` opcode). Unsupported pattern kinds — `Range`,
    `TupleStruct`, `Struct`, `Path`, `Or`, `Rest` — emit a typed
    `EmitErrorKind::UnsupportedFeature { what: "<kind> in match" }`
    pointing at the offending arm; other arms still emit so partial
    diagnostics remain useful.
  - **Hygiene**: arm-local bindings cannot leak. The emitter
    snapshots `self.locals` before each arm and restores it after,
    so two arms can introduce a binding with the same name without
    tripping `DuplicateLocal`. The `locals_count` grows
    monotonically — each binding occupies a fresh slot — so the
    function's declared `locals_count` still matches the wire
    contract.
  - **Verifier**: no changes. The new lowering reuses only the
    existing opcodes (`StoreLocal`, `LoadLocal`, `LoadConst`, `Eq`,
    `JumpIfFalse`, `Jump`, `LoadNone`), all of which are already
    covered by the S5c verifier and its goldens. The unreachable
    `LoadNone`/`next_arm_N` instructions that appear after a
    wildcard arm are tolerated by the verifier's worklist
    (unreachable code is not visited).
  - **CLI**: `capyc run` now executes source-level `match`
    expressions out of the box.
  - **Tests**: 5 new emitter end-to-end tests in
    `crates/capy-emitter/tests/end_to_end.rs`
    (`match_literal_arms_lower_to_eq_and_jump_chain`,
    `match_ident_pattern_binds_and_uses_in_body`,
    `match_guard_branches_to_next_arm`,
    `match_arm_bindings_do_not_leak_across_arms`,
    `match_unsupported_pattern_emits_typed_error`); 8 new VM
    end-to-end tests in `crates/capy-vm/tests/end_to_end.rs`
    covering literal selection, wildcard fall-through, ident
    binding, guard true/false, non-exhaustive → `None`, `let v =
    match …;`, and nested `match`.
  - **README**: `S2.2b match + patterns` flips from "frontend done;
    emitter pending" to "first cut done (literal + wildcard +
    ident bindings; range / struct / or-patterns deferred)".

- **S5c follow-up** — verifier goldens + docs audit.
  - New `crates/capy-bytecode/tests/verify_goldens.rs` exercises
    `verify_function` from outside the crate with a table-driven
    surface: one golden per stable diagnostic code `B0013`-`B0020`
    plus the principal success shapes (straight-line arithmetic,
    `if`-then-`else` balanced, `while` loop with back-edge, direct
    `Call`, S7 `HostCall` with and without arguments).
  - Two HostCall-specific goldens: `host_call_underflow_propagates_b0013`
    pins the verifier's stack-discipline treatment of `HostCall`
    against `argc`-too-large; `host_call_does_not_check_import_idx_at_verify_time`
    documents the deliberate additive choice to defer `import_idx`
    bounds checking to runtime (V0014) so the existing
    `verify_function` signature stays additive within v0.
  - New `stable_codes_match_documented_catalogue` test pins the
    `VerifyError` → `code()` mapping to the `B0013`-`B0020` strings
    documented in `docs/compatibility.md` and `docs/bytecode-v0.md`,
    so any future renumbering breaks loudly rather than silently
    drifting the cross-repo ABI.
  - Cross-repo doc audit: `docs/compatibility.md` "Validation
    before CapyOS integration" list refreshed (S4 / S6 / S7 are
    no longer "when X lands"; the verifier goldens, the VM
    end-to-end tests and the in-process `HostAdapter` reference
    are now called out explicitly).
  - `docs/integration.md` gains a new "Reference adapter
    (host-test only)" section clarifying that the in-process
    `HostAdapter` shipped by S7 is a host-test artefact, not the
    CapyOS integration surface; CapyOS continues to own the real
    adapter and bind against the `(module, symbol)` pairs in the
    bytecode `Imports` section.

- **S7 (emitter lowering)** — `Item::Import` items lower into
  `HostCall` at every matching call site.
  - **ModuleEmitter** pass 1 now collects both `fn` indices AND
    `import_index: HashMap<callable_name, u32>` in source order. The
    callable name is the `as` alias when present, else the import
    path's last segment. The wire `(module, symbol)` pair still
    reflects the underlying import path, so the host bridge surface
    stays decoupled from local renaming.
  - **FunctionEmitter** receives `import_index` alongside `fn_index`.
    A new internal `CallTarget` enum routes each `Expr::Call` to
    either `Instruction::Call { fn_idx, argc }` (in-module) or
    `Instruction::HostCall { import_idx, argc }` (imported). Local
    `fn` items shadow imports of the same name.
  - **Error catalogue** extends with `E0020 DUPLICATE_IMPORT`
    (`EmitErrorKind::DuplicateImport { name }`) — emitted when two
    imports collapse to the same callable name; first wins, second
    is reported and skipped.
  - **Behaviour change** (additive at the source level, breaking on
    the wire only for callers who previously relied on the
    accidental alias-as-symbol behaviour, which was never reachable
    without a host adapter): `import a::b as c` now produces
    `Import { module: "a", symbol: "b" }`, not
    `Import { module: "a", symbol: "c" }`. The existing
    `imports_pass_through` end-to-end test was updated accordingly.
  - **Tests**: 6 new end-to-end tests in
    `crates/capy-emitter/tests/end_to_end.rs` (`imported_call_lowers_to_host_call`,
    `imported_call_with_args_forwards_argc`,
    `import_alias_renames_callable_but_keeps_wire_symbol`,
    `local_fn_shadows_import_of_same_name`,
    `duplicate_import_name_is_reported_first_wins`,
    `unknown_callee_is_still_reported`) plus 3 in
    `crates/capy-vm/tests/end_to_end.rs` exercising the full
    source → emitter → VM-with-HostAdapter pipeline
    (`imported_call_runs_through_host_adapter`,
    `import_alias_lets_source_use_local_name`,
    `unresolved_import_traps_deterministically_at_runtime`).

- **S7 (sketch)** — host bridge: `host_call` opcode + in-process
  adapter.
  - **Bytecode**: new `Opcode::HostCall = 0x82` with `U32U32`
    immediate `(import_idx, argc)`, mnemonic `host_call`,
    `Instruction::HostCall { import_idx, argc }`, full encode/
    decode/disassemble support. Additive within v0; the existing
    opcode set (0x00..=0x81) is unchanged. `docs/bytecode-v0.md`
    table extended with the new row + a paragraph describing the
    dispatch contract.
  - **Verifier**: `HostCall` participates in the stack-discipline
    pass with `required = argc`, `delta = 1 - argc`. Static
    `import_idx` range-checking is deferred to runtime (see V0014
    below) so the existing `verify_function` signature stays
    additive.
  - **VM**: new `HostAdapter` module (`crates/capy-vm/src/host.rs`)
    publishing `HostAdapter`, `HostFn` (`fn(args: &[Value]) ->
    HostResult`), `HostResult = Result<Value, &'static str>` and
    two deterministic built-in stubs: `time::now` (returns
    `Value::Int(0)`) and `log::info` (accepts a single
    `Value::Str`, returns `Value::None`). Helper
    `HostAdapter::with_builtin_stubs()` returns an adapter pre-
    populated with both. `Vm::from_module` keeps its existing
    empty-adapter behaviour; a new `Vm::from_module_with_host`
    accepts an adapter, and `Vm::with_host_adapter` swaps one in
    after load.
  - **VmError**: three new fail-closed variants with stable codes
    `V0014 UNKNOWN_HOST_IMPORT` (out-of-range `import_idx`),
    `V0015 UNRESOLVED_HOST_IMPORT` (no handler registered) and
    `V0016 HOST_CALL_FAILED` (handler returned `Err(reason)`).
    All three are additive inside the v0 VM ABI.
  - **CLI**: `capyc run` now wires `HostAdapter::with_builtin_stubs()`
    automatically so a precompiled module that exercises
    `time::now` / `log::info` runs out of the box.
  - **Decoupling preserved**: zero kernel headers, zero syscalls,
    zero raw pointers across the boundary. Handlers see opaque
    `Value`s only; reasons must be `&'static str`. CapyOS still
    owns the real adapter; this sketch is the in-process mock the
    test harness uses today.
  - **Tests**: 6 new unit tests under `capy-vm` (dispatch,
    unknown-import, unresolved-symbol, arg-forwarding,
    handler-error surfacing, default-empty-adapter trap) plus 5
    tests in `capy-vm::host` covering the registry and the two
    built-in stubs.
  - **Emitter unchanged**: source-level lowering of imported
    function calls remains a future slice. Today, modules that
    exercise `host_call` hand-craft their `Imports` section and
    bytecode (the new VM tests are the canonical examples).

- **S12 (sketch)** — `capyc` unified CLI front-end.
  - New `crates/capyc/` binary crate publishing the `capyc`
    executable (added to the workspace members list).
  - Five subcommands wrapping the toolchain stages:
    - `capyc tokens [PATH]` — prints the canonical lexer dump
      (the existing golden artefact);
    - `capyc parse [PATH]` — prints `dump_source` for the AST;
    - `capyc compile [PATH] [-o OUTPUT]` — emits v0 bytecode
      bytes (writes to stdout when `-o` is absent);
    - `capyc disasm [PATH]` — loads a v0 bytecode file and
      prints constants / functions (with `disassemble_text`) /
      imports / debug sections;
    - `capyc run [PATH] [--entry NAME] [--budget N]` — accepts
      either source or a precompiled bytecode file (auto-
      detected via the `CAPY` magic), compiles in-memory if
      needed, executes via `Vm::run_with_budget` and prints
      the return value.
  - `PATH = -` (or omitted) reads from standard input;
    `--help` / `-h` and `--version` / `-V` are honoured at the
    top level and per subcommand.
  - Exit-code contract mirrors `capyc-tokens`: `0` clean,
    `1` recoverable diagnostics surfaced by any stage,
    `2` usage / I/O / loader error.
  - No external dependencies; argument parsing is hand-rolled
    to stay lock-step with `capyc-tokens` and the workspace
    `rust-version = 1.75` floor.
  - Diagnostic rendering uses a friendly blurb table for
    `B0001`-`B0020` and falls through to the stable code
    string for everything else so the CLI never silently drops
    information.

- **S2.2b** — `match` expressions and pattern grammar (frontend-only).
  - **AST** (`capy_ast`): new `Expr::Match { scrutinee, arms, span }`,
    `MatchArm { pattern, guard, body, span }`, `Pattern` enum
    (`Wildcard`, `Rest`, `Literal`, `Ident`, `Path`, `TupleStruct`,
    `Struct`, `Or`, `Range`, `Error`) and `StructPatternField`.
    `Expr::is_block_like` now also returns true for `Expr::Match`
    so `match` may be used at statement position without a trailing
    `;`. The canonical AST dump grows the new node kinds
    `Match`, `Arm`, `Scrutinee`, `Guard`, `Body`, `PatWildcard`,
    `PatRest`, `PatLiteral`, `PatIdent`, `PatPath`,
    `PatTupleStruct`, `PatStruct` (with optional ` ..` trailer for
    a rest field), `PatField`, `PatOr`, `PatRange` (with `..`
    or `..=` trailer) and `PatError`. Field order and the
    `[start..end]` prefix follow the S2.0 dump contract.
  - **Parser** (`capy_parser`): `match <scrutinee> { <arm,>* }`
    arms have the shape `<pattern> [if <guard>] => <body>`. Arms
    are separated by `,`; a trailing `,` is allowed and the `,`
    is optional after an arm whose body is block-like (Rust
    convention). Or-patterns use `|`; range patterns recognise
    `..` (exclusive) and `..=` (inclusive, detected by spatial
    adjacency of the `..` and `=` tokens since no dedicated lexer
    token exists). Patterns recover into `Pattern::Error` and a
    typed `ParseErrorKind::UnexpectedToken` diagnostic; the
    parser never aborts.
  - **Emitter** (`capy_emitter`): `match` is intentionally not
    lowered yet. Encountering it now produces a deterministic
    `EmitErrorKind::UnsupportedFeature { what: "match expressions" }`
    with code `E0010` (existing `E_UNSUPPORTED_FEATURE`). Lowering
    lands in a later S2.x sub-slice once the emitter learns
    pattern compilation.
  - **Lexer**: no new tokens. Pre-existing `Match`, `FatArrow`,
    `Pipe`, `DotDot`, `DotDotDot`, `Colon`, `ColonColon` plus the
    `Ident` regex (which captures `_`) cover the full grammar.
  - **Goldens**: three new fixtures under
    `crates/capy-parser/tests/fixtures/parser/` —
    `19_match_basic.cl` (literal + wildcard arms),
    `20_match_guard_path.cl` (tuple-struct + guard + path + None
    literal + negative literal) and
    `21_match_or_range_struct.cl` (or-pattern with inclusive
    range + struct pattern with rest).
  - **Docs**: `docs/grammar.ebnf` gains a dedicated S2.2b section
    formalising `match_expr`, `match_arm`, the pattern grammar
    and the `..=` adjacency rule. `block_like_expr` includes
    `match_expr`.

- **S5c** — Static stack-balance verifier.
  - New `capy_bytecode::verify_function(instructions, locals_count,
    callee_locals_counts)` walks a function's CFG from entry and
    proves: (1) no operand-stack underflow on any reachable
    instruction; (2) all control-flow predecessors of every
    reachable instruction agree on the stack depth on entry;
    (3) every reachable path terminates via `Return` with exactly
    one operand on the stack; (4) every `LoadLocal` / `StoreLocal`
    slot fits in the declared `locals_count`; (5) every `Jump` /
    `JumpIfFalse` target lands on a real instruction boundary
    inside the function; (6) every `Call (fn_idx, argc)` references
    a known function and respects the callee's locals window.
  - Wired into `Vm::from_module`: malformed bytecode is now rejected
    at load time rather than trapping mid-execution, surfacing as
    `VmError::MalformedModule { code, reason }` with the verifier's
    stable diagnostic code. The corresponding runtime traps
    (`UnknownFunctionIndex`, `CallArityMismatch`) become belt-and-
    suspenders: they remain defined and reachable only via direct
    interpreter usage outside `from_module`.
  - Returns a `VerifyReport { max_depth }` on success — kept for a
    future budget-aware stack pre-sizing pass.
  - New stable codes (bytecode side, additive within v0):
    `B0013 VERIFIER_STACK_UNDERFLOW`,
    `B0014 VERIFIER_STACK_INCONSISTENCY`,
    `B0015 VERIFIER_FALL_OFF_END`,
    `B0016 VERIFIER_INVALID_RETURN_DEPTH`,
    `B0017 VERIFIER_LOCAL_OUT_OF_BOUNDS`,
    `B0018 VERIFIER_JUMP_OUT_OF_BOUNDS`,
    `B0019 VERIFIER_UNKNOWN_FUNCTION_INDEX`,
    `B0020 VERIFIER_CALL_ARITY_OVERFLOW`.
  - Closes the S5b.2 caveat about `break` / `continue` reached from
    inside a partially evaluated expression: any such program that
    produces a stack-inconsistent join point is now rejected at
    load time rather than executing with stale stack residue.
  - Drive-by repair of a pre-existing structural defect in
    `crates/capy-bytecode/src/error.rs` where the `MalformedInstruction`
    variant was syntactically nested inside `MalformedDebug`'s field
    list; the two are now declared as sibling enum variants. This is
    a no-op for the public catalogue (`B0012` retained its code) but
    was required for the crate to compile cleanly under the verifier
    additions.

- **S5b.3** — Function calls and parameters, end-to-end across
  bytecode, emitter and VM.
  - **Bytecode**: new opcode `call (0x80)` with the `U32U32`
    immediate shape `(fn_idx, argc)`. Adds the third immediate
    shape to the v0 codec and bumps the documented opcode count
    from 24 to 25; renaming or reusing the byte value remains
    breaking. The full table is reflected in
    `docs/bytecode-v0.md`.
  - **Emitter**: two-pass module emission — pass 1 reserves a
    stable function-table slot per `fn` (using a benign
    `LoadNone; Return` placeholder so per-function emission
    failures cannot shift any other index), pass 2 lowers
    bodies in source order. Forward and backward calls now
    resolve. Parameters are registered as the first locals of
    each function (`locals[0..argc]`, matching the `Call`
    ABI), so the previously rejected `fn add(x, y) { x + y }`
    shape lowers correctly. New stable codes
    `E0016 UNKNOWN_FUNCTION`, `E0017 UNSUPPORTED_CALLEE`,
    `E0018 DUPLICATE_FUNCTION`, `E0019 TOO_MANY_ARGUMENTS`.
    Direct calls to top-level identifiers only; path callees
    (e.g. `foo::bar()`), field/index callees and dynamic values
    are rejected with `UnsupportedCallee`.
  - **VM**: `Vm` keeps a call-frame stack. `Call` pops `argc`
    arguments into the callee's `locals[0..argc]`, switches to
    the callee, and `Return` pushes the popped top-of-stack
    back into the caller's operand stack. A deterministic
    `MAX_CALL_DEPTH = 256` limit traps unbounded recursion as
    `CallStackOverflow`; mismatched arity traps as
    `CallArityMismatch`; out-of-bounds `fn_idx` traps as
    `UnknownFunctionIndex`. New stable codes
    `V0011 CALL_STACK_OVERFLOW`, `V0012 UNKNOWN_FUNCTION_INDEX`,
    `V0013 CALL_ARITY_MISMATCH`.

- **S5b.2** — Emitter extensions for control-flow and short-circuit
  boolean operators, completed without introducing new opcodes (the v0
  instruction set frozen by S5a remains unchanged).
  - `while <cond> <body>` lowers to a header-poll loop using
    `JumpIfFalse` for the cond test and a single `Jump` back-edge.
    The expression always evaluates to `None`; a `break <value>`
    inside has its payload discarded at the join point.
  - `loop <body>` lowers to an unconditional `Jump` back-edge. The
    expression's value is the payload of the `break` that terminates
    the loop (or `None` when `break` carries no value).
  - `break [<value>]` / `continue` consult an emitter-local loop
    stack and emit a single `Jump` to the innermost loop's break /
    continue label. Used outside any enclosing loop, they produce
    typed `E0014 BREAK_OUTSIDE_LOOP` / `E0015 CONTINUE_OUTSIDE_LOOP`
    diagnostics and the offending function is dropped from the
    module (fail-closed).
  - `&&` and `||` lower to short-circuit branch sequences built on
    `JumpIfFalse` + `LoadTrue` / `LoadFalse`. The right-hand side
    is **not** evaluated when the left-hand side already determines
    the result (verified end-to-end against a trapping
    `10 / 0 == 1` rhs).
  - New stable emitter codes: `E0014 BREAK_OUTSIDE_LOOP`,
    `E0015 CONTINUE_OUTSIDE_LOOP`.

- **S6.1** — VM core: deterministic stack machine that interprets the
  v0 instruction set. New `capy-vm` crate completes the first
  **source → tokens → AST → bytecode → execution → Value** pipeline.
  - `Vm::from_module(&[u8]) -> Result<Vm, VmError>` parses the v0
    container, decodes the constant pool and function table, pre-
    decodes every function's instruction stream and pre-computes a
    `byte_offset → instruction_index` map so jump targets resolve in
    O(1).
  - `Vm::run(fn_name) -> Result<Value, VmError>` /
    `Vm::run_with_budget(fn_name, budget)` execute the named entry
    point on a fresh evaluation stack and locals frame; the budget
    decrements on every executed instruction and traps fail-closed
    on exhaustion. `DEFAULT_INSTRUCTION_BUDGET = 1_000_000`.
  - `Value` enum: `None`, `Bool`, `Int(i64)`, `Float(f64)`,
    `Str(String)`. Owned strings (clone-on-store) for v0; an
    `Rc<str>` refinement is a future slice.
  - Determinism contract (documented in the crate root): wrapping
    integer arithmetic (no overflow panics), IEEE-754 floats, strict
    same-type binary ops with the single exception of `Int <-> Float`
    promotion across the numeric category, strict `Bool` for
    `JumpIfFalse`/`Not`, deterministic error returns and no global
    state. Running the same compiled module twice with the same
    inputs returns identical `Value` / `VmError`.
  - Stable error catalogue (frozen for S6.1): `V0001`
    `STACK_UNDERFLOW`, `V0002` `LOCAL_OUT_OF_BOUNDS`, `V0003`
    `CONST_OUT_OF_BOUNDS`, `V0004` `JUMP_OUT_OF_BOUNDS`, `V0005`
    `TYPE_MISMATCH`, `V0006` `DIVISION_BY_ZERO`, `V0007`
    `BUDGET_EXHAUSTED`, `V0008` `UNKNOWN_FUNCTION`, `V0009`
    `MALFORMED_MODULE` (wraps the originating `B<NNNN>` code via
    its `reason`/`code` payload), `V0010` `EXPECTED_BOOL`.
    `VmError::code()` returns the stable `&'static str` for each
    variant. The catalogue is purely additive within v0; the
    previously reserved `V<NNNN>` prefix in
    `docs/compatibility.md` is now first occupied by S6.1.
  - Opcode coverage: every S5a opcode is implemented and tested.
    `Nop`/`Pop`/`Load*`/`Store*` perform the obvious stack
    manipulations; `Add`/`Sub`/`Mul`/`Div`/`Mod`/`Neg` honour
    wrapping i64 semantics and Int/Float promotion;
    `Eq`/`Ne` accept matching primitive types (with Int/Float
    cross promotion) and trap fail-closed on category mismatches;
    `Lt`/`Le`/`Gt`/`Ge` are numeric-only;
    `Not` is `Bool`-only; `Jump`/`JumpIfFalse` resolve targets
    through the precomputed offset map and trap on past-end /
    non-instruction targets; `Return` pops the top of the stack
    (or returns `None` for an empty stack).
  - 7 new unit tests in `src/execute.rs` (low-level
    pool-and-function fixtures): integer addition, unknown
    function, integer division by zero, integer overflow wrap,
    budget exhaustion threshold, `int + bool` type mismatch,
    `JumpIfFalse` rejects non-bool.
  - 17 new integration tests in `tests/end_to_end.rs` running
    actual source through the full pipeline: empty function,
    multi-precedence arithmetic, comparison operators (`==`,
    `!=`, `<`, `<=`), `let` + use (single and two-local), `if`
    then-branch, `if` else-branch, nested `if`, explicit
    `return`, early `return` inside `if`, unary `-`/`!`, float
    arithmetic, `Int + Float` promotion, plain string literal,
    string with `\n` escape, runtime division by zero, and a
    repeat-execution determinism check.
- `docs/compatibility.md` and `README.md` updated to reflect the
  new S6.1 ABI surface and the first complete source-to-value
  pipeline.

- **S5b.1** — AST → bytecode emitter. New `capy-emitter` crate provides
  `emit(&Source) -> EmitOutput { module, errors }` lowering for the
  subset of the frontend that the v0 instruction set can already
  express: literals, locals, paren, unary `Neg`/`Not`, arithmetic and
  comparison binary operators, blocks, `let`, expression statements,
  `if`/`else`, `return` and the top-level `Item::Fn` (no params) /
  `Item::Import` forms. The output is a fully self-consistent
  `Module` that round-trips through `Module::parse`.
  - `crates/capy-emitter/` — new workspace crate with three internal
    modules: `ConstPoolBuilder` (dedup-aware, indexed by value/bits/
    string), `FunctionEmitter` (per-function byte buffer + label table
    + pending-jump patch list), `ModuleEmitter` (top-level iteration
    + `EmitOutput` assembly).
  - Stable error catalogue (frozen for S5b.1): `E0001` `UNSUPPORTED_EXPR`,
    `E0002` `UNSUPPORTED_ITEM`, `E0003` `UNKNOWN_LOCAL`, `E0004`
    `DUPLICATE_LOCAL`, `E0005` `INTEGER_PARSE`, `E0006` `FLOAT_PARSE`,
    `E0007` `STRING_PARSE`, `E0008` `UNSUPPORTED_BINARY`, `E0009`
    `UNSUPPORTED_UNARY`, `E0010` `UNSUPPORTED_FEATURE`, `E0011`
    `PARSE_ERROR_IN_EXPR`, `E0012` `TOP_LEVEL_MUST_BE_ITEM`, `E0013`
    `NESTED_ITEM`. `EmitError::code()` returns the stable
    `&'static str` for each variant.
  - Literal parsing: integers accept decimal, `0x`, `0b`, `0o` with
    underscore separators (parsed as `i64`); floats accept decimal +
    `e` exponent with underscores (`f64`); strings strip the outer
    quotes and decode the minimal escape set `\n` `\t` `\r` `\\` `\"`
    `\0`. Other escapes are rejected fail-closed.
  - Lowering invariants: every `Expr` lowering pushes exactly one
    value; every `Stmt` is stack-neutral (expression statements emit
    a trailing `Pop` regardless of `has_semi`); `Block` produces
    exactly one value (its tail expression or `LoadNone` when no
    tail is present); function bodies wrap the body's value with
    `Return`. Jumps are patched after the body is fully lowered via
    a label table + pending-jump list, so `if`/`else` produces the
    canonical `JumpIfFalse <else>`/`Jump <end>`/`<else>:`/`<end>:`
    layout.
  - Per-item failure isolation: if a function body fails to lower,
    the function is omitted from the output and an `EmitError` is
    pushed to `errors`. Other functions continue to be emitted so
    partial output is still meaningful.
  - 8 new unit tests covering literal parsers (decimal/hex/bin/oct
    integers, decimal+exponent floats, escapeless strings, escape
    decoding, unknown-escape rejection) and the const-pool deduper
    (int dedup, float dedup via bit pattern, `-0.0` vs `0.0`
    distinction).
  - 9 new integration tests in `tests/end_to_end.rs` exercising the
    full source → AST → bytecode → instruction-stream pipeline:
    empty fn, simple arithmetic tail, `let` + ident, `if`/`else`,
    explicit `return` with dead-code wrapper, const-pool dedup
    across a function, import pass-through (with and without `as`
    alias), unsupported-fn-with-params error path, and a full
    `Module::serialize` → `Module::parse` round-trip.
- `docs/compatibility.md` and `README.md` updated to reflect the
  new S5b.1 ABI surface and the closed source-to-bytes pipeline.

- **S5a** — v0 opcode set + instruction encoder/decoder. Frozen contract
  for the instruction stream that lives inside `Function.code`. The
  bytecode emitter (S5b) and the VM (S6-S9) will consume this same
  surface.
  - `capy-bytecode::opcode::Opcode` — 24 opcodes covering stack
    manipulation (`Nop`, `Pop`), constants (`LoadConst`, `LoadTrue`,
    `LoadFalse`, `LoadNone`), locals (`LoadLocal`, `StoreLocal`),
    arithmetic (`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`), comparison
    (`Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`), logical (`Not`), control
    flow (`Jump`, `JumpIfFalse`) and function termination (`Return`).
    `Opcode::from_byte` / `as_byte` / `immediate` / `mnemonic` are the
    stable surface; the numeric byte values are part of the
    `capy-lang-host` v0 ABI.
  - `Imm { None, U32, I32 }` describes the immediate shape per opcode
    (`U32` for indices, `I32` for relative jump offsets). Jump offsets
    are PC-relative to the byte that follows the full instruction.
  - `capy-bytecode::instruction::Instruction` — typed enum with one
    variant per opcode, carrying the typed immediate where applicable
    (`LoadConst(u32)`, `LoadLocal(u32)`, `StoreLocal(u32)`,
    `Jump(i32)`, `JumpIfFalse(i32)`).
  - `encode(&[Instruction]) -> Vec<u8>` / `decode(&[u8]) ->
    Result<Vec<Instruction>, BytecodeError>` round-trip the stream
    deterministically. `disassemble_text(&[Instruction]) -> String`
    emits the stable `<offset:04x>  <mnemonic>  <imm?>` format used by
    debug tooling.
  - New error code `B0012` `MALFORMED_INSTRUCTION` covers unknown
    opcode bytes and truncated immediates inside a function's `code`
    slice.
  - 14 new unit tests: opcode byte round-trip across all 24 variants,
    unique-byte invariant, immediate-width table, instruction full
    round-trip, predictable encoding of `1 + 2`, unknown-opcode
    rejection, truncated `U32` immediate, truncated `I32` immediate,
    negative jump offset, deterministic disassembly text (including
    negative-offset rendering).
  - 2 new integration tests in `tests/instruction_pipeline.rs`
    exercise instructions → `Function.code` → `FunctionTable` →
    `Module` round-trip, including a branch-shaped jump pattern.
- `docs/bytecode-v0.md` *functions* section now includes the full
  *Instruction set (frozen by S5a)* table with mnemonics, immediate
  shapes and stack effects. Error table extended with `B0012`.

- **S4b** — per-section typed encoders/decoders for the four body
  sections defined in S4. Additive within `capy-lang-host` v0; the
  existing `Section` framing surface is unchanged (raw payload round
  trips still work).
  - `capy-bytecode` (extended): new public types
    `ConstPool { entries: Vec<Constant> }`,
    `Constant { Int(i64), Float(f64), Str(String) }`,
    `FunctionTable { entries: Vec<Function> }`,
    `Function { name, locals_count, code }`,
    `ImportTable { entries: Vec<Import> }`,
    `Import { module, symbol }`,
    `DebugInfo { entries: Vec<DebugEntry> }`,
    `DebugEntry { bytecode_offset, source_start, source_end }`.
    Each type ships `encode() -> Vec<u8>` and
    `decode(&[u8]) -> Result<Self, BytecodeError>` that mirror the
    layout documented in `docs/bytecode-v0.md`.
  - `capy-bytecode::cursor` (private) — minimal bounds-checked byte
    cursor shared by the four payload decoders.
  - Extended error catalogue (frozen): `B0008` `MALFORMED_CONSTANTS`,
    `B0009` `MALFORMED_FUNCTIONS`, `B0010` `MALFORMED_IMPORTS`,
    `B0011` `MALFORMED_DEBUG`. Each new variant carries
    `{ offset: usize, reason: &'static str }` and is exposed via
    `BytecodeError::code()`.
  - Decoder validation (fail-closed): unknown const tags, invalid
    UTF-8 in strings, truncated counts/lengths, length overflows,
    trailing bytes after the declared count and inverted source
    spans (`source_end < source_start`) are all rejected with a
    typed `BytecodeError`.
  - 20 new unit tests in `consts` (empty pool, Int+Float+Str
    round-trip, unknown const tag, invalid UTF-8, truncated payload,
    trailing bytes, `-0.0` bit-pattern preservation), `functions`
    (empty table, single function, order preservation across 3,
    truncated count, invalid UTF-8 name, trailing bytes), `imports`
    (empty/single/multi round-trip, invalid UTF-8 module, truncated
    symbol) and `debug` (empty round-trip, order, inverted span,
    trailing bytes). Plus 2 new integration tests
    (`tests/typed_round_trip.rs`) exercising the full
    typed → bytes → `Module` → bytes → typed pipeline.
- `docs/bytecode-v0.md` *Body* section now documents the exact
  per-section payload schemas (constants, functions, imports, debug)
  and the full `B0001`-`B0011` error-code catalogue. Header table
  unchanged.

- **S4** — bytecode v0 container (`capy-bytecode` crate). Implements the
  **frozen** 32-byte header layout documented in `docs/bytecode-v0.md`
  and the body framing for the four section tags. Additive within
  `capy-lang-host` v0.
  - `crates/capy-bytecode/` — new workspace crate. Public surface:
    `Module { abi_version, sections }` (deterministic
    `serialize`/`parse`), `Header { bc_version, abi_version, flags,
    body_length, checksum }` (parse/serialise the 32-byte little-endian
    layout exactly), `Section { tag, payload }` with `SectionTag`
    enum (`Consts=0x01`, `Functions=0x02`, `Imports=0x03`,
    `Debug=0x04`), `parse_sections` framing parser, `Checksum`
    type alias and `compute_checksum` (BLAKE3-128 via the `blake3`
    crate), constants `HEADER_SIZE = 32`, `MAGIC = b"CAPY"`,
    `MAX_SUPPORTED_BC_VERSION = 0`, `CHECKSUM_SIZE = 16`,
    `SECTION_HEADER_SIZE = 5`.
  - Stable bytecode diagnostic code catalogue (frozen for S4):
    `B0001` `MAGIC_MISMATCH`, `B0002` `UNSUPPORTED_BC_VERSION`,
    `B0003` `RESERVED_FLAGS_NONZERO`, `B0004` `BODY_LENGTH_MISMATCH`,
    `B0005` `CHECKSUM_MISMATCH`, `B0006` `MALFORMED_SECTION`,
    `B0007` `TRUNCATED_HEADER`. `BytecodeError` carries a stable
    `code() -> &'static str` for each variant.
  - Loader is **fail-closed**: validates magic, rejects unknown
    `bc_version` (only `0` accepted at v0), rejects non-zero `flags`,
    verifies `body_length` matches the trailing byte count and
    verifies BLAKE3-128 of the body before parsing section content.
    Unknown section tags, truncated section headers and section
    payloads that overflow the body are rejected with
    `MalformedSection { offset, reason }`.
  - Serialisation is **deterministic**: identical `Module` values
    produce identical bytes; `body_length` and `checksum` are
    recomputed from the body on every `serialize`.
  - 23 new unit tests across `header` (round-trip, truncated input,
    wrong magic, unknown major, non-zero flags, 32-byte invariant),
    `checksum` (reference BLAKE3-128 of empty input matches the
    published value `af1349b9f5f9a1a6a0404dea36dcc949`,
    avalanche on single-byte change), `section` (tag round-trip,
    empty body, single section, multiple sections in order,
    unknown tag, truncated header, payload overflow) and `module`
    (empty round-trip, full round-trip with all four section types,
    corrupted body fails checksum, truncated body fails length
    check, magic at byte 0, unknown major inside module,
    deterministic serialisation).
  - Single external dependency: `blake3 = "1"` (pure Rust, audited,
    `no_std`-friendly). Cargo.lock will be repopulated by the first
    `cargo build` on CI.
- `docs/bytecode-v0.md` `Header` table remains the authoritative
  specification; no field added, removed or reordered.

- **S3** — structured diagnostics + source map + rustc-style renderer.
  New `capy-diagnostics` crate. Additive within `capy-lang-host` v0; the
  existing per-stage diagnostic types (`capy_lexer::Diagnostic`,
  `capy_parser::ParseDiagnostic`) are preserved and bridged into the
  unified shape.
  - `crates/capy-diagnostics/` — new workspace crate. Public surface:
    `Severity` (`Error`, `Warning`, `Note`, `Help`), `Code` (stable
    `&'static str` wrapper), `Label { span, message }`, `Diagnostic`
    (severity + code + message + primary label + secondary labels +
    notes), `SourceMap` (byte → `(line, col)` translation with
    `O(n)` construction and `O(log n)` queries; CRLF aware in
    `line_text`), `render(diag, source, file) -> String` producing
    a deterministic rustc-style text block with header, location,
    gutter, source line and caret line.
  - Stable code catalogue (frozen for S3): `L0001`
    (`L_UNTERMINATED_STRING`), `L0002`
    (`L_UNTERMINATED_BLOCK_COMMENT`), `L0003` (`L_UNKNOWN_CHAR`),
    `P0001` (`P_UNEXPECTED_TOKEN`), `P0002`
    (`P_UNEXPECTED_EOF`). Prefix scheme reserved for future stages:
    `B<NNNN>` (bytecode S4), `V<NNNN>` (VM S6-S9), `H<NNNN>` (host
    ABI S11).
  - `bridge::from_lex` and `bridge::from_parse` deterministic
    conversions; `ParseErrorKind::Lex` is recursively routed back
    through `from_lex` so each lex error always renders with the
    same code regardless of which stage surfaced it.
  - 11 new unit tests across `source_map`, `render` and `bridge`,
    including exact text snapshots for the canonical
    `unterminated string`, `unexpected token`, `unexpected EOF` and
    second-line span cases.
- `docs/grammar.ebnf` and `docs/compatibility.md` updated to reflect
  the new S3 ABI surface and the frozen code catalogue.

- **S2.3b** — `type`, `enum`, `import` items + dedicated `Type` AST node.
  Closes the syntactic frontend for the language-core subset.
  - `capy-ast`: new `Type` enum (`Path { segments, span }`,
    `Error { span }`) is now a first-class AST node — replaces the
    previous `Expr` placeholder in `Param.ty`, `FnItem.ret_ty`,
    `ConstItem.ty`, `StructField.ty` and `Stmt::Let.ty`. New items
    `Item::TypeAlias(TypeAlias)`, `Item::Enum(EnumItem)`,
    `Item::Import(ImportItem)` with supporting types `Variant`,
    `VariantBody { Unit, Tuple(Vec<Type>), Struct(Vec<StructField>) }`.
    `Type::span()` mirrors `Expr::span()` shape.
  - `capy-ast::dump_source`: new lines `[..] TypePath "..."` /
    `[..] TypeError` for types, `[..] Item TypeAlias "<name>"` with
    `Type` sub-label, `[..] Item Enum "<name>"` followed by
    `[..] Variant "<name>"` (Unit), `... Tuple` (with `TypePath` rows)
    or `... Struct` (with `Field` rows), and `[..] Item Import
    "<path>" [as "<alias>"]` (single flat line). Existing fixtures
    (`02_let_typed`, `06_fn`, `07_const`, `08_struct`) updated where
    type annotations migrated from `Ident` to `TypePath`; no other
    span or label changed.
  - `capy-parser`: new internal `parse_type` (path types only;
    fail-closed with `Type::Error` recovery on non-`Ident`); new
    `parse_type_alias_item`, `parse_enum_item`, `parse_variant`,
    `parse_import_item`; shared `parse_struct_field_list` reused by
    struct and struct-like enum variants; `token_starts_item()`
    extended with `Type`/`Enum`/`Import`. All existing parsers
    (`parse_let_stmt`, `parse_fn_item`, `parse_param_list`,
    `parse_const_item`, `parse_struct_item`) now route type
    annotations through `parse_type`.
  - `crates/capy-parser/tests/fixtures/source/` — 4 new byte-precise
    goldens (`09_type_alias`, `10_enum`, `11_import`, `12_import_as`).
    4 existing source goldens updated where the type-annotation lines
    moved from `Ident` to `TypePath`.
  - 6 new unit tests covering type alias, unit-variant enum,
    payload-variant enum (tuple + struct + unit mixed), import
    without alias, import with alias, and `parse_type` recovery on a
    non-identifier (numeric literal).
- `docs/grammar.ebnf`: new active *S2.3b type/enum/import + dedicated
  type grammar* section; the `type_ann = expression` placeholder
  removed; `let_stmt`, `fn_item`, `param`, `const_item`,
  `struct_field` now reference `type` directly. New productions
  `type_alias_item`, `enum_item`, `variant`, `variant_payload`,
  `tuple_types`, `import_item`, `import_path`, `type`, `type_path`.

- **S2.3a** — top-level and block-level item declarations: `fn`, `const`,
  `struct`. Additive within `capy-lang-host` v0; no S2.0-S2.2 surface
  renamed or removed.
  - `capy-ast`: new `Item` enum (`Fn(FnItem)`, `Const(ConstItem)`,
    `Struct(StructItem)`), supporting types `FnItem`, `Param`,
    `ConstItem`, `StructItem`, `StructField`; new `Stmt::Item(Item)`
    variant so items can interleave with `let`/`expr` statements at
    every level (top-level `Source` and inside any `Expr::Block`).
    All new types carry a `span` field; `Item::span()` and updated
    `Stmt::span()` route accordingly. `body` of `FnItem` is typed as
    `Box<Expr>` and always points to an `Expr::Block`.
  - `capy-ast::dump_source` extended with `Item Fn "<name>"` /
    `Item Const "<name>"` / `Item Struct "<name>"` lines plus
    sub-labels `Params` / `Param "<name>"` / `RetType` / `Body`
    (for `fn`), `Type` / `Init` (for `const`), `Field "<name>"` (for
    `struct`). Existing dump format unchanged for all S2.0-S2.2
    nodes.
  - `capy-parser`: new helper `token_starts_item()`, new internal
    parsers `parse_item`, `parse_fn_item`, `parse_param_list`,
    `parse_const_item`, `parse_struct_item`. Top-level and block
    loops dispatch on item keywords before falling back to `let` or
    expression statements. Item bodies/fields/params/return types
    reuse the existing `parse_expression(MIN_PREC)` for type
    annotations (placeholder until S2.3's dedicated type grammar
    lands).
  - `crates/capy-parser/tests/fixtures/source/` — 3 new byte-precise
    goldens (`06_fn`, `07_const`, `08_struct`) covering a function
    with params and return type, a typed constant and a struct with
    multiple fields.
  - 6 new unit tests covering each item kind, empty parameter lists,
    empty struct fields, no-return-type form and interleaving of
    items with `let`/`expr` statements.
- `docs/grammar.ebnf`: new active *S2.3a Item declarations* section
  formalising `item_stmt`, `fn_item`, `param_list`, `param`,
  `const_item`, `struct_item`, `struct_field_list`, `struct_field`.
  `stmt` extended with `item_stmt` alternative. *Item grammar
  placeholder* replaced by S2.3a definitions; remaining items
  (`enum`/`trait`/`impl`/`type`/`import`/patterns) reanchored to
  S2.3b/c.

- **S2.2** — control flow expressions. Additive within `capy-lang-host`
  v0; no existing S2.0 or S2.1 surface renamed or removed.
  - `capy-ast`: 6 new `Expr` variants — `If { cond, then_branch,
    else_branch, span }` (then-branch is always an `Expr::Block`;
    else-branch is either another `If` for `else if ...` chains or a
    `Block` for `else { ... }`), `While { cond, body, span }`,
    `Loop { body, span }`, `Return { value: Option<Box<Expr>>, span }`,
    `Break { value: Option<Box<Expr>>, span }`, `Continue { span }`.
    `Expr::is_block_like()` extended to include `If`, `While`, `Loop`
    (so they may appear as statements without `;`); `Return`, `Break`,
    `Continue` intentionally remain non-block-like.
  - `capy-parser`: new helper `token_starts_expression()` used by the
    optional value-detection in `return` / `break`; new internal
    parsers `parse_if_expr` (recursive on `else if`), `parse_while_expr`,
    `parse_loop_expr`, `parse_return_expr`, `parse_break_expr`,
    `parse_continue_expr`, all hooked into `parse_primary` as new
    primary kinds.
  - `crates/capy-parser/tests/fixtures/parser/` — 7 new expression
    fixtures (`12_if_else`, `13_while`, `14_loop`, `15_return`,
    `16_break`, `17_continue`, `18_if_else_if`).
  - `crates/capy-parser/tests/fixtures/source/` — 1 new source fixture
    (`05_if_stmt`) demonstrating that `if`-statement at top level does
    not require `;` because the expression is block-like.
  - 9 new unit tests in `capy-parser` covering each control-flow
    primary, `else-if` chain recursion, value-less and value-carrying
    `return`, the `;` requirement for `continue` as a statement and
    the no-`;` form of `if` as a top-level statement.
- `docs/grammar.ebnf`: new active *S2.2 Control flow primaries* section;
  `primary_expr` extended with `if_expr`, `while_expr`, `loop_expr`,
  `return_expr`, `break_expr`, `continue_expr`; `block_like_expr` now
  includes `if_expr | while_expr | loop_expr` in addition to
  `block_expr`.

- **S2.1** — statements, blocks and top-level `Source`. Additive within
  `capy-lang-host` v0; no existing S2.0 surface renamed or removed.
  - `capy-ast`: new `Stmt` enum (`Let { name, ty, init, span }`,
    `Expr { expr, has_semi, span }`), new `Source { stmts, span }`
    top-level node, new `Expr::Block { stmts, tail, span }` variant
    (Rust-style: optional trailing expression without `;` becomes the
    block value), `Expr::is_block_like()` predicate (S2.1: `Block` only;
    extended by S2.2 control flow), `dump_source` canonical text format
    re-using the same `[start..end] Kind` line shape as `dump_expr` plus
    explicit sub-labels `Type` / `Init` (for `Let`) and `Tail` (for
    `Block`).
  - `capy-parser`: new `parse_source(&str) -> ParseSourceResult` entry,
    new internal `parse_block_expr` / `parse_let_stmt` /
    `parse_expr_stmt` / `parse_top_level_stmts`, block recognised as a
    new `primary_expr` (`{` ... `}`). Same fail-closed contract: missing
    `;` and missing `}` are recoverable, produce typed diagnostics, do
    not panic, never abort the stream. Defensive bump in the top-level
    and block loops guarantees forward progress even when a sub-parser
    cannot consume the offending token.
  - `crates/capy-parser/tests/fixtures/parser/` — 2 new expression
    fixtures (`10_block_expr`: `{ let x = 1; x + 1 }`, `11_block_unit`:
    `{ let x = 1; }`).
  - `crates/capy-parser/tests/fixtures/source/` — new fixture directory
    with 4 byte-precise goldens (`01_let`, `02_let_typed`,
    `03_two_stmts`, `04_recover_missing_semi`) driven by a new
    `tests/golden_source.rs` integration test that exercises
    `parse_source` + `dump_source`.
  - 5 new unit tests in `capy-parser` covering empty block, block with
    tail, single `let`, missing-`;` recovery and multi-statement source.
- `docs/grammar.ebnf`: new active *S2.1 Statements, blocks and top-level
  source* section formalising `source`, `block_expr`, `tail_expr`,
  `stmt`, `let_stmt`, `expr_stmt`, `block_like_expr`. `program` rerouted
  to `source`; *Item grammar* placeholder reanchored to S2.x.

- **S2.0** — expression parser slice. Two new workspace crates and one new
  golden-test harness; ABI extension is additive within `capy-lang-host` v0.
  - `crates/capy-ast/` — span-preserving AST. Public surface: `Expr` (with
    variants `Int`, `Float`, `Str`, `Bool`, `NoneLit`, `Ident`, `Path`,
    `Paren`, `Call`, `Index`, `Field`, `Unary`, `Binary`, `Error`),
    `Ident`, `UnOp` (`Neg`, `Not`, `BitNot`), `BinOp` (18 operators
    covering arithmetic, bitwise, shift, comparison, equality and logical),
    `BinOp::precedence()` and `BinOp::as_str()` as stable mnemonics, plus
    `dump_expr` producing the canonical AST dump used by golden tests.
    Re-exports `capy_lexer::Span` so the lexer/parser pipeline shares a
    single span type.
  - `crates/capy-parser/` — hand-written recursive-descent parser.
    Public surface: `parse_expr(&str) -> ParseResult { expr, diagnostics }`,
    `ParseDiagnostic`, `ParseErrorKind` (`UnexpectedToken`,
    `UnexpectedEof`, `Lex`). Trivia tokens are filtered before parsing;
    lexer diagnostics are forwarded as `ParseErrorKind::Lex`. Parser
    follows the same fail-closed contract as the lexer: never panics on
    user input, every malformed range produces a typed diagnostic and an
    `Expr::Error` placeholder so consumers can still walk the tree.
    Precedence-climbing for binary operators is left-associative with the
    10-level C-like table documented in `docs/grammar.ebnf`.
  - `crates/capy-parser/tests/fixtures/parser/` — 9 byte-precise golden
    fixtures (`01_int`, `02_precedence`, `03_left_assoc`, `04_call`,
    `05_path`, `06_index_field`, `07_unary_paren`,
    `08_recover_unexpected`, `09_unterminated_string`) exercising
    literals, precedence, left-associativity, calls with trailing commas,
    multi-segment paths, postfix chains and recovery from both
    unexpected-token and lexer-level errors. Anti-drift gate in the Rust
    CI workflow rejects accidental regeneration, mirroring the lexer
    convention.
- `docs/grammar.ebnf`: lexical grammar section unchanged; new active
  *S2.0 Expression grammar* section formalising expression /
  precedence-climbing / postfix / path / primary productions. The
  *Statement and item grammar* placeholder is preserved and re-anchored
  to future S2.x sub-slices.
- Workspace `Cargo.toml`: members list now declares `capy-ast`,
  `capy-lexer`, `capy-parser`, `capyc-tokens` (alphabetical). `Cargo.lock`
  updated with the two new packages.

### Planned

- **S2.2b**: `match` expressions with patterns.
- **S2.3c**: `trait` + `impl` items, patterns, plus extended type forms
  (tuple, function, array, reference, generic, trait-object).
- **S5b.2**: control-flow extensions to the emitter — `while`, `loop`,
  `break`, `continue` with proper stack-discipline analysis;
  short-circuit `&&` / `||` lowering via `JumpIfFalse` / `Jump`.
- **S5b.3**: function `Call u32` opcode + parameter binding + return
  value passing; emitter support for fn parameters, fn calls and
  `import`-bound symbol calls.
- **S12**: `capyc check` front-end powered by `capy-diagnostics::render`.

## [0.1.3] - 2026-05-20

Cross-repo synchronisation with CapyOS `0.8.0-alpha.244+20260520`. No code
change in the lexer or CLI; ABI surface (`capy-lang-host` v0 partial, S1) is
unchanged and remains additive within v0. The CapyOS-side audit
(`compatibility-audit-2026-05-20.md`) already pins CapyLang `0.1.3`.

### Changed

- `docs/compatibility.md`: pin CapyOS core `0.8.0-alpha.241+20260519` ->
  `0.8.0-alpha.244+20260520`; point the audit reference at
  `compatibility-audit-2026-05-20.md`; expand the *Owned ABI*, *Error model*,
  *Resource and performance limits*, *Install/update boundary*, *Dependency
  rules*, *Validation*, *Publishing as a Capy package*, *Local development*,
  *Continuous integration* and *Integration rule* sections so that the
  CapyLang side mirrors the cross-repo contract surface CapyOS now enforces.
- `docs/integration.md`: refresh the pin line to
  `0.8.0-alpha.244+20260520`.
- `README.md`, `docs/lexer.md`: bump the advertised version to `0.1.3`.
- `Cargo.toml` `workspace.package.version` and the corresponding
  `Cargo.lock` entries for `capy-lexer` and `capyc-tokens` move from `0.1.2`
  to `0.1.3` (previous release commit accidentally left the workspace
  manifest at `0.1.2`).

## [0.1.2] - 2026-05-19

Cross-repo synchronisation release. No lexer code change, no public API
change, no fixture regeneration. Strictly a pin bump.

### Changed

- `docs/compatibility.md` and `docs/integration.md`: pin CapyOS core
  `0.8.0-alpha.240+20260519` -> `0.8.0-alpha.241+20260519`.
- `README.md`, `docs/lexer.md`: bump the advertised version to `0.1.2`.
- `Cargo.toml` `workspace.package.version` and the corresponding
  `Cargo.lock` entries to `0.1.2`.

## [0.1.1] - 2026-05-19

Polish round on top of S1: additive public API, richer CLI, broader fixture
coverage and contributor-facing docs. No breaking changes.

### Added

- `TokenKind::is_trivia()` - stable `const fn` predicate. The trivia set
  (`Whitespace`, `Newline`, `LineComment`, `BlockComment`) becomes part of
  the public contract; removing a kind from it is a breaking change.
- `capyc-tokens --counts`: deterministic histogram `<count> <Kind>` sorted
  by count descending, ties broken by kind-name ascending. Stable output
  regardless of `HashMap` iteration order.
- `capyc-tokens --no-trivia`: filters trivia from the output of both
  `dump` and `counts` modes. The synthetic `Eof` token is always
  retained; diagnostics are preserved so the exit code remains stable.
- Six additional golden fixtures (total now 23):
  - `18_string_escapes` - escaped quote and backslash inside a string;
  - `19_keyword_boundary` - `fn` vs `fnord`, `fn_x`, `_fn` (regression
    for keyword/identifier priority tie-break);
  - `20_only_trivia` - whitespace, tab, line and nested block comments;
  - `21_string_raw_newline` - raw newline aborts the literal, lexer
    recovers and produces a second `Error` on the unmatched closing quote;
  - `22_realistic_fn` - `fn add(x: i32, y: i32) -> i32 { x + y }`;
  - `23_decls_keywords` - `pub struct ... enum ... trait ... impl ...
    type ... const`.
- 8 additional unit tests in `capyc-tokens` covering the new flags, the
  trivia filter and the deterministic histogram formatter.
- `CONTRIBUTING.md`: contribution workflow, lint policy, fixture-authoring
  checklist, integration-boundary reminders, PR checklist.
- `docs/lexer.md`: new *Token predicates* section, fixture count refreshed
  (17 -> 23), CLI examples for `--counts` and `--no-trivia`.

### Changed

- `README.md` *Status* section now lists the polished S1 capabilities
  (23 fixtures, the two `TokenKind` predicates, `capyc-tokens` CLI).
- `README.md` *Local development* block documents the new CLI invocations
  and the exit-code contract (0 clean, 1 diagnostics, 2 usage/I/O).

## [0.1.0] - 2026-05-19

First slice (**S1**) of the production compiler: a deterministic, lossless
lexer in Rust with byte-precise spans and recoverable diagnostics.

### Added

- `capy-lexer` crate (logos 0.14 backbone) producing 60+ token kinds covering
  the Phase-1 grammar: trivia, all keywords (`fn`, `let`, `const`, `if`,
  `else`, `match`, `try`, `catch`, `throw`, `async`, `await`, `arena`, ...),
  literals (decimal/hex/binary/octal integers with `_` separators, float with
  optional exponent, string with escapes), Unicode identifiers via
  `\p{XID_Start}` / `\p{XID_Continue}`, every punctuation and operator
  required for the parser slice.
- Lossless trivia: whitespace, newlines, line comments and arbitrarily nested
  block comments are emitted as their own tokens so downstream tooling
  (formatter, syntax highlighter, parser with trivia preservation) can
  reconstruct the source byte-for-byte.
- Error recovery contract: every malformed range becomes a
  `TokenKind::Error` token paired with a classified `Diagnostic`
  (`UnterminatedString`, `UnterminatedBlockComment`, `UnknownChar`). The
  lexer never aborts; the stream always ends with a synthetic
  `TokenKind::Eof` token at byte `[len, len)`.
- Public API: `tokenize`, `Lexer` iterator, `dump_tokens`, `TokenKind`,
  `Span`, `Token`, `Diagnostic`, `LexErrorKind`.
- `capyc-tokens` debugging CLI: prints the canonical token dump for a file
  or standard input; exits `0` when clean, `1` when diagnostics are present,
  `2` on usage / I/O errors.
- Golden-test harness with 17 fixtures (`crates/capy-lexer/tests/fixtures/lexer/`)
  covering empty input, every literal radix, keywords vs identifiers,
  Unicode identifiers, nested block comments, unterminated string and block
  comment recovery, all compound operators, arrows and path separators,
  nested brace blocks. The harness fails CI when the canonical dump drifts.
- `make rust-validate` aggregating `fmt-check`, `clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and
  `cargo test --workspace --doc`.
- GitHub Actions `Rust` workflow caching the cargo registry, running fmt,
  clippy, target tests and doctests, and rejecting accidental golden updates
  via `git diff` after the test phase.
- `docs/grammar.ebnf` with the complete lexical grammar (active for S1) and
  a reserved placeholder for the syntactic grammar landing in S2.
- `docs/bytecode-v0.md` with the frozen v0 header layout (magic `CAPY`,
  version, abi_version, flags, body_length, BLAKE3-128 checksum) and the
  invariants required by the CapyOS-Etapa-15 contract.
- `.gitattributes` enforcing `eol=lf` on every text file so byte-precise
  goldens behave identically on Windows checkouts with `core.autocrlf=true`.

### Defensive measures

- `slice_safe` helper used by both the lexer and the dumper avoids logos's
  `unsafe { from_utf8_unchecked }` slice when the recovery cursor lands in
  the middle of a multi-byte UTF-8 sequence. Regression test
  `unmatched_multibyte_does_not_panic` covers it.
- Per-module `#![forbid(unsafe_code)]` everywhere except the file that hosts
  the `#[derive(Logos)]` expansion (which may emit `unsafe` blocks for
  branchless lookup tables).
- Workspace lints: `clippy::all` at warn level; CI promotes warnings to
  errors via `RUSTFLAGS="-D warnings"`.

### Project scaffolding

- Workspace `Cargo.toml` (resolver 2, MSRV 1.75, edition 2021) with
  centralised package metadata and lint tables.
- `rust-toolchain.toml` pinning the stable channel plus the `rustfmt` and
  `clippy` components.
- Bumped `VERSION` from `0.0.1` to `0.1.0` and refreshed `README.md` with
  an *Active layout* section reflecting the new Rust crates.

[Unreleased]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.3...v0.1.6
[0.1.3]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/henriquefarisco/CapyLang/releases/tag/v0.1.0
