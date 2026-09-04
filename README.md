# CapyLang

Version: 0.1.12

CapyLang is the external language-core repository for CapyOS.

## Status

Slice **S1 (lexer)** is implemented and polished under `crates/capy-lexer`,
with a thin `capyc-tokens` debug CLI living in `crates/capyc-tokens`. The
lexer is span-tracking, produces recoverable diagnostics, ships 23 golden
fixtures and exposes two stable `TokenKind` predicates (`carries_text`,
`is_trivia`).

Slice **S2.0 (expression parser)** is implemented under `crates/capy-ast`
(span-preserving AST) and `crates/capy-parser` (hand-written
recursive-descent parser with C-like operator precedence, postfix chains,
trivia-skipping over the lexer stream and the same fail-closed recovery
contract: every malformed range becomes an `Expr::Error` paired with a
typed `ParseDiagnostic`).

Slice **S2.1 (statements, blocks, top-level source)** extends the AST
with `Stmt` (`Let`, `Expr { has_semi }`) and `Source { stmts, span }`,
adds `Expr::Block { stmts, tail }` as a new primary expression, and
exposes `parse_source` as the top-level entry.

Slice **S2.2 (control flow)** adds 6 new `Expr` variants — `If` (with
recursive `else-if` chain), `While`, `Loop`, `Return`, `Break`,
`Continue` — and integrates them as primary expressions. `If`,
`While` and `Loop` extend `Expr::is_block_like()` and may stand as
statements without `;`; `Return`, `Break` and `Continue` still
require `;` in statement position.

Slice **S2.3a (item declarations)** adds `fn`, `const`, `struct` as
both top-level and block-level declarations via a new `Item` AST node
and `Stmt::Item(Item)` variant; items can interleave freely with
`let` and expression statements.

Slice **S2.3b (`type`/`enum`/`import` + dedicated `Type`)** closes
the syntactic frontend. `Type` is now a first-class AST node (path
types in S2.3b; tuple/function/array/reference/generic types deferred
to S2.3c). `Item::TypeAlias`, `Item::Enum` (with unit / tuple /
struct-like variants) and `Item::Import` (with optional `as` alias)
ship.

Slice **S2.2b (`match` + patterns, frontend-only)** adds
`Expr::Match { scrutinee, arms }`, `MatchArm { pattern, guard, body }`
and a `Pattern` AST (wildcard, rest, literal — incl. negative
numeric — identifier binding, multi-segment path, tuple-struct,
struct with `..` rest field, or-pattern and `..` / `..=` range).
`match` is block-like; the arm `,` is mandatory between non-block-
like bodies and optional after block-like bodies. The emitter ships
a **second cut** that lowers `_` (wildcard), identifier bindings,
literal patterns (incl. negative numeric literals), range patterns
(`..` exclusive via `Lt`, `..=` inclusive via `Le`) and or-patterns
(`alt0 | alt1 | …`) into the existing v0 opcode set — no new wire
surface. Arm-local binding hygiene is preserved via a save/restore
of the locals map; the scrutinee lives in a synthetic local
allocated through `alloc_unnamed_local`. Identifier bindings inside
or-pattern alts are restricted (binding-merging is type-aware and
lands later). Tuple-struct, struct and path patterns still produce
a typed `EmitErrorKind::UnsupportedFeature { what: "<kind> in
match" }` pointing at the offending arm. Three golden fixtures under
`crates/capy-parser/tests/fixtures/parser/` cover the principal
parser shapes and the emitter/VM `end_to_end` test suites pin the
lowering and runtime semantics.

Slice **S3 (structured diagnostics)** introduces the
`capy-diagnostics` crate: `Severity`, stable `Code` catalogue
(`L0001`-`L0003` for the lexer, `P0001`-`P0002` for the parser),
`Label`, `Diagnostic` (with primary + secondary labels and notes),
`SourceMap` for byte → `(line, col)` translation and a deterministic
rustc-style `render` function. `bridge::from_lex` and
`bridge::from_parse` map the existing per-stage diagnostics into the
unified shape without surface drift.

Slice **S4 (bytecode v0 container)** introduces the
`capy-bytecode` crate: the frozen 32-byte header (magic `CAPY`,
`bc_version`, `abi_version`, `flags`, `body_length`,
BLAKE3-128 `checksum`), body framing (`tag(u8) + length(u32 LE) +
payload`) for the four section tags (`Consts`, `Functions`,
`Imports`, `Debug`), a deterministic `Module::serialize` /
`Module::parse` round-trip and the `B0001`-`B0007` error catalogue.

Slice **S4b (typed section payloads)** layers per-section
encoders/decoders on top of S4: `ConstPool` (`Int(i64)` /
`Float(f64)` / `Str(String)`), `FunctionTable`
(`name`, `locals_count`, opaque `code`), `ImportTable`
(`module::symbol`) and `DebugInfo` (`bytecode_offset` ↔ source
byte range). Each type ships `encode` / `decode` with fail-closed
validation; the error catalogue extends to `B0001`-`B0011`.

Slice **S5a (v0 opcode set + instruction codec)** freezes the
instruction stream that lives inside `Function.code`: 24 opcodes
covering stack/constants/locals/arithmetic/comparison/control flow
plus `Return`, with three immediate shapes (`None`, `U32`, `I32`).
`Instruction` is the typed enum, `encode` / `decode` round-trip
deterministically, `disassemble_text` is the stable debug format.
Error code `B0012` covers unknown opcodes and truncated immediates.

Slice **S6.1 (VM core)** introduces the `capy-vm` crate and closes
the **first complete source → tokens → AST → bytecode → execution
pipeline**: `Vm::from_module(bytes).run("main")` returns a `Value`.
The interpreter is a deterministic stack machine over the S5a
instruction set, with wrapping i64 arithmetic, IEEE-754 floats,
strict same-type binary ops (with `Int ↔ Float` promotion across
the numeric category), strict `Bool` for `JumpIfFalse`/`Not`, a
per-call instruction budget (default `1_000_000`) and a fail-closed
error catalogue `V0001`-`V0019` (S5b.3 added `V0011`-`V0013` for
calls; S7 added `V0014`-`V0016` for host calls; S6.2 added `V0017`
`INDEX_OUT_OF_BOUNDS`; S6.3a added `V0018` `FIELD_OUT_OF_BOUNDS`;
S6.2b added `V0019` `POP_EMPTY_ARRAY`). No
JIT, no syscalls, no host pointers, no global state.

Slice **S5c (static stack-balance verifier)** runs at VM load time
(`Vm::from_module`) and statically rejects any function whose
operand-stack discipline cannot be proven. It checks no
underflow, no path-disagreement on stack depth, `Return` with
exactly one operand, in-bounds locals, jump targets landing on
real instruction boundaries, and `Call`s with known `fn_idx` and
`argc` ≤ callee's `locals_count`. Stable codes `B0013-B0020`.
Closes the S5b.2 caveat about `break` / `continue` reached from
inside a partially evaluated expression: any such program with a
stack-inconsistent join point is now rejected before execution
begins.

Slice **S5b.3 (function calls + parameters)** adds the v0 `call`
opcode (`0x80`, `(fn_idx, argc)` `U32U32` immediate), a two-pass
module emitter that resolves forward and backward calls and registers
parameters as the first locals of each function, and a VM call-frame
stack with a deterministic `MAX_CALL_DEPTH = 256` recursion guard plus
runtime arity and `fn_idx` validation. Direct calls to top-level
functions only — path / field / dynamic callees are still rejected.

Slice **S5b.2 (control-flow + short-circuit booleans)** completes the
emitter coverage of the v0 opcode set: `while` / `loop` lower to a
deterministic header-poll / unconditional back-edge pattern, `break`
and `continue` consult an emitter-local loop stack and emit a single
`Jump` (with typed `E0014 BREAK_OUTSIDE_LOOP` / `E0015
CONTINUE_OUTSIDE_LOOP` diagnostics when used outside any loop), and
`&&` / `||` lower to short-circuit branch sequences that provably
skip the right-hand side when the left-hand side already determines
the result. No new opcodes — the v0 instruction set frozen by S5a
remains unchanged.

Slice **S5b.1 (AST → bytecode emitter)** introduces the
`capy-emitter` crate and closes the **first end-to-end pipeline**:
`source → tokens → AST → bytecode bytes`. `emit(&Source)` lowers
literals (with proper integer base + escape handling), locals,
paren, unary `-`/`!`, arithmetic + comparison binaries, blocks,
`let`, expression statements, `if`/`else` and `return` into the
S5a opcodes; top-level `fn` (no params yet) and `import` items are
turned into `FunctionTable` and `ImportTable` entries. Constants
are interned, jumps are patched after the body is fully lowered,
and the produced `Module` round-trips through `Module::parse`.
Error catalogue `E0001`-`E0019` (S5b.2 added `E0014` / `E0015` for
break/continue outside any enclosing loop; S5b.3 added `E0016`
`UNKNOWN_FUNCTION`, `E0017` `UNSUPPORTED_CALLEE`, `E0018`
`DUPLICATE_FUNCTION` and `E0019` `TOO_MANY_ARGUMENTS`).

Slice **S2.4 (assignment + mutation)** adds `Expr::Assign { target,
value }` to the AST and parses `=` (plus the compound forms `+=`, `-=`,
`*=`, `/=`, `%=`, desugared to `target = target <op> value`) at the
lowest precedence, right-associative. The emitter lowers an assignment
to a local into `<value>` + `StoreLocal` + `LoadNone` (the expression
evaluates to unit), so reassignable locals make imperative loops such as
`while i <= n { total = total + i; i = i + 1; }` expressible end-to-end.
Only a simple local identifier is assignable in v0; other targets raise
the typed `E0021 INVALID_ASSIGN_TARGET`, and assigning to an undeclared
name reuses `E0003 UNKNOWN_LOCAL`.

Slice **S5d (integer bitwise & shift operators)** wires `&`, `|`, `^`,
`<<`, `>>` and unary `~` (parsed since S2.0 but previously rejected by
the emitter) through the whole pipeline. Six opcodes join the v0 set
additively — `band` (0x36), `bor` (0x37), `bxor` (0x38), `shl` (0x39),
`shr` (0x3A) and `bnot` (0x51). They operate on integers only (a
non-integer operand traps with `V0005 TYPE_MISMATCH`) and shift counts
are reduced modulo 64, so the VM stays total and deterministic.

Slice **S6.2 (arrays)** adds the first aggregate value: array literals
`[a, b, c]`, indexing `a[i]`, indexed assignment `a[i] = v` and four
opcodes (`make_array` 0x60, `array_get` 0x61, `array_set` 0x62,
`array_len` 0x63). `Value::Array` has reference semantics (an
`Rc<RefCell<Vec<Value>>>`) so a bound array is mutated in place; element
access is bounds-checked and a bad index traps with `V0017
INDEX_OUT_OF_BOUNDS`. This unblocks variable-length data such as a grid
or the snake body.

Slice **S6.2b (growable arrays)** makes those arrays *resizable* with two
additive opcodes (`array_push` 0x67, `array_pop` 0x68) in the reserved
aggregate block. `array_push` appends a value to a bound array in place
(reference semantics, growing it by one) and pushes the same handle back;
`array_pop` removes and returns the last element. A push/pop on a
non-array traps with `V0005 TYPE_MISMATCH`, and popping an empty array
traps fail-closed with the new `V0019 POP_EMPTY_ARRAY`. This is bytecode +
VM only (testable by building modules directly); the `a.push(x)` /
`a.pop()` frontend surface follows with the method-call/stdlib work. It
completes the variable-length building block the snake body needs.

Slice **S6.2c (array insert / remove)** rounds out the growable surface
with two positional opcodes (`array_insert` 0x69, `array_remove` 0x6A) in
the same reserved aggregate block. `array_insert` pops `(arr, idx, val)`,
inserts `val` at `idx` in place (shifting later elements right, growing by
one; `idx == len` appends exactly like `array_push`) and pushes the same
handle back; `array_remove` pops `(arr, idx)`, removes the element at `idx`
(shifting later elements left, shrinking by one) and pushes it. Both reuse
the index trap `V0017 INDEX_OUT_OF_BOUNDS` for a negative or out-of-range
index (no new trap) and `V0005 TYPE_MISMATCH` on a non-array operand. Like
S6.2b this is bytecode + VM only; the `a.insert(i, x)` / `a.remove(i)`
frontend surface follows with the method-call/stdlib work.

Slice **S6.3a (tagged aggregates)** adds the runtime value model for
`struct` / `enum`: `Value::Aggregate { tag, fields }` (same
reference-semantics `Rc<RefCell<Vec<Value>>>` backing as arrays) plus
three opcodes (`make_aggregate` 0x64, `get_field` 0x65, `get_tag` 0x66).
`make_aggregate (tag, field_count)` builds a struct instance or enum
variant from the top `field_count` operands; `get_field` clones a field
out (out-of-range traps with `V0018 FIELD_OUT_OF_BOUNDS`); `get_tag`
pushes the opaque discriminant so a lowered `match` can branch. This
sub-slice is bytecode + VM only — the frontend/emitter lowering
(enum construction, struct literals, the `match` aggregate arms) lands
in S6.3b / S6.3c with no new opcodes.

Slice **S6.3b (enum construction + `match`)** wires the emitter onto that
value model. An `enum` declaration registers each variant's tag; a
unit variant written as a path (`Color::Red`) lowers to
`make_aggregate(tag, 0)`, a tuple variant call (`Some(5)`) emits its
arguments then `make_aggregate(tag, argc)`, and `match` arms for unit
(`Color::Red`) and tuple (`Some(x)`) variants lower to a `get_tag`
discriminant test plus recursive `get_field` extraction that binds or
tests each payload. No new opcodes; `struct` literals and `Struct`
patterns follow in S6.3c. So `enum Box { Val(Int), Empty }` with
`match Box::Val(7) { Box::Val(x) => x, Box::Empty => 0 }` now evaluates
to `7` end-to-end.

Slice **S6.3c (structs)** completes the data model. A `struct` declaration
registers a field layout; a struct literal `Point { y: 2, x: 1 }` is
lowered with its fields reordered into declaration order, and a `match`
`Struct` pattern destructures by field name. The parser only treats
`Path { ... }` as a struct literal outside the head of `if` / `while` /
`match` and the bounds of `for` (where the `{` opens a block) — Rust's
rule. So `struct Point { x: Int, y: Int }` with
`let p = Point { x: 3, y: 4 }; match p { Point { x, y } => x + y }`
evaluates to `7`. Field access `p.x` stays deferred to the type checker.

## Scope owned by this repository

- Parser and syntax validation.
- Bytecode or IR format.
- VM/interpreter core.
- Minimal standard library independent from CapyOS internals.
- Host runner and mock host ABI.
- Golden tests for parser, bytecode and VM behavior.
- Deterministic benchmark programs such as `Snake` and `Asteroids` when the VM is mature enough.

## Out of scope

- Direct CapyOS syscalls.
- Kernel pointers or CapyOS internal structs.
- CapyOS filesystem, compositor, input or timer access without host ABI.
- JIT in the first integration wave.

## CapyOS integration contract

CapyOS core pinned: `0.10.0-alpha.1+20260903`.

CapyOS integration must follow:

- `CapyOS/docs/reference/integration/capylang-integration-contract.md`
- `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`
- `CapyOS/docs/reference/integration/modular-installation-architecture.md`
- `CapyOS/docs/reference/integration/compatibility-matrix.md`
- `CapyOS/docs/reference/integration/capypkg-publisher-manifest-format.md`
- `CapyOS/docs/operations/manual-module-deploy-runbook.md`
- `docs/compatibility.md`

## Active layout

```text
crates/
  capy-ast/               # S2 - span-preserving AST (Expr, BinOp, UnOp, dump_expr)
    src/
      lib.rs              # public API re-exports
      expr.rs             # Expr enum, Ident, BinOp, UnOp (precedence table)
      dump.rs             # canonical AST text dump for goldens
  capy-lexer/             # S1 - lexical analysis (logos-based)
    src/
      lib.rs              # public API re-exports
      token.rs            # TokenKind + logos derive
      lexer.rs            # Lexer wrapper, Span, Token, tokenize()
      diagnostic.rs       # Diagnostic, LexErrorKind
      dump.rs             # canonical text dump for goldens
    tests/
      golden.rs           # golden test harness
      fixtures/lexer/     # 23 .cl + .tokens pairs
  capy-bytecode/          # S4 / S4b - v0 container + typed section payloads
    src/
      lib.rs              # public API re-exports
      header.rs           # frozen 32-byte header (parse/serialize, validation)
      checksum.rs         # BLAKE3-128 wrapper (Checksum, compute_checksum)
      section.rs          # SectionTag, Section, parse_sections framing
      module.rs           # Module = header + sections (deterministic round-trip)
      consts.rs           # ConstPool / Constant (Int, Float, Str) encode/decode
      functions.rs        # FunctionTable / Function (name, locals, opaque code)
      imports.rs          # ImportTable / Import (module::symbol)
      debug.rs            # DebugInfo / DebugEntry (bytecode -> source span)
      opcode.rs           # Opcode enum (43 frozen byte values) + Imm shape
      instruction.rs      # Instruction enum + encode/decode/disassemble_text
      cursor.rs           # private bounds-checked byte cursor
      error.rs            # BytecodeError + B0001..B0012 codes
    tests/
      typed_round_trip.rs # typed -> bytes -> Module -> typed integration test
      instruction_pipeline.rs # instructions -> Function -> Module round-trip
  capy-emitter/           # S5b.1 - AST -> bytecode lowering
    src/
      lib.rs              # public API re-exports
      emit.rs             # ModuleEmitter + FunctionEmitter + literal parsers
      error.rs            # EmitError + E0001..E0013 codes
    tests/
      end_to_end.rs       # source -> AST -> bytecode integration tests
  capy-vm/                # S6.1 - deterministic stack-machine interpreter
    src/
      lib.rs              # public API re-exports
      value.rs            # Value (None/Bool/Int/Float/Str/Array/Aggregate)
      execute.rs          # Vm loader + interpreter loop
      error.rs            # VmError + V0001..V0019 codes
    tests/
      end_to_end.rs       # source -> AST -> bytecode -> Value integration tests
  capy-diagnostics/       # S3 - Severity, Code, Label, Diagnostic, SourceMap, render, bridges
    src/
      lib.rs              # public API re-exports
      diagnostic.rs       # Severity, Code, Label, Diagnostic, code catalogue
      source_map.rs       # byte -> (line, col) translation
      render.rs           # rustc-style text renderer
      bridge.rs           # from_lex / from_parse conversions
  capy-parser/            # S2.0 .. S2.3b - recursive-descent parser
    src/
      lib.rs              # public API re-exports (parse_expr, parse_source, ...)
      diagnostic.rs       # ParseDiagnostic, ParseErrorKind
      parser.rs           # expressions + stmts + control flow + items + types
    tests/
      golden.rs           # expression AST golden test harness
      golden_source.rs    # top-level Source AST golden test harness
      fixtures/parser/    # 18 .cl + .ast pairs (expressions, blocks, control flow)
      fixtures/source/    # 12 .cl + .ast pairs (statements + items + types)
  capyc-tokens/           # S1 - debug CLI (dump / counts / no-trivia modes)
    src/main.rs
  capyc/                  # S12 - unified CLI (tokens/parse/check/compile/disasm/run/repl)
    src/main.rs
docs/
  compatibility.md        # ABI and sandbox contract (pre-existing)
  integration.md          # CapyOS Etapa 15 boundary (pre-existing)
  grammar.ebnf            # lexical + (planned) syntactic grammar
  bytecode-v0.md          # bytecode container spec (header frozen by S4)
  lexer.md                # S1 contract: tokens, predicates, recovery, dump
CHANGELOG.md              # Keep-a-Changelog, anchors each release to a slice
CONTRIBUTING.md           # workflow, lint policy, PR checklist
```

## Planned layout (Fase 1 closure)

```text
crates/
  capy-lexer/      # S1   (done)
  capy-ast/        # S2.0 (done)
  capy-parser/     # S2.0  expressions (done)
                   # S2.1  statements / blocks / top-level Source (done)
                   # S2.2  control flow if/while/loop/return/break/continue (done)
                   # S2.3a items fn/const/struct (done)
                   # S2.3b items type/enum/import + dedicated Type AST (done)
                   # S2.2b match + patterns (second cut: literal/wildcard/ident/range/or done;
                   #         tuple-struct/struct/path deferred)
                   # S2.3c trait/impl + extended type forms (planned)
  capy-diagnostics/# S3   (done)
  capy-bytecode/   # S4   header + section framing (done)
                   # S4b  per-section typed encoders/decoders (done)
                   # S5a  opcode set + instruction codec (done)
                   # S5c  static stack-balance verifier (done)
  capy-emitter/    # S5b.1 AST → bytecode emitter (done)
                   # S5b.2 control-flow extensions (done)
                   # S5b.3 Call opcode + params (done)
                   # S7    import lowering → HostCall (done)
                   # S2.2b match lowering (second cut: literal/wildcard/ident/range/or)
  capy-vm/         # S6.1 stack-machine interpreter (done)
                   # S5b.3 call frames + recursion guard (done)
                   # S5c  load-time verifier wiring (done)
                   # S7 host_call opcode + HostAdapter (sketch done)
                   # S8-S9 real host ABI modules (planned)
  capy-stdlib/     # S10
  capy-host-abi/   # S11
  capyc/           # S12 unified CLI (tokens/parse/check/compile/disasm/run/repl) (done)
benchmarks/
  snake/           # S13
```

## Local development

CapyLang ships a `rust-toolchain.toml` pinning the `stable` channel. Install
once with [rustup](https://rustup.rs) and then:

```bash
make build           # cargo build --workspace --all-targets
make test-rust       # cargo test  --workspace --all-targets + --doc
make fmt-check       # cargo fmt --all -- --check
make clippy          # cargo clippy --workspace --all-targets -- -D warnings
make rust-validate   # fmt-check + clippy + test-rust
make update-goldens  # CAPY_GOLDEN_UPDATE=1 cargo test (rewrites .tokens)
make validate        # legacy doc/policy gates (no Rust required)
```

Debug CLI for the lexer:

```bash
cargo run -p capyc-tokens -- crates/capy-lexer/tests/fixtures/lexer/10_complete.cl
echo 'let x = 42' | cargo run -p capyc-tokens
cargo run -p capyc-tokens -- --counts crates/capy-lexer/tests/fixtures/lexer/22_realistic_fn.cl
cat example.cl | cargo run -p capyc-tokens -- --no-trivia --counts
```

Unified toolchain CLI (`capyc`, S12 sketch):

```bash
# Print tokens / AST for a source file.
cargo run -p capyc -- tokens example.cl
cargo run -p capyc -- parse  example.cl

# Static check: rustc-style diagnostics (with source carets) for the
# lexer, parser and emitter stages. Exit 1 if any diagnostic is shown.
cargo run -p capyc -- check  example.cl

# Compile to bytecode and disassemble it back.
cargo run -p capyc -- compile example.cl -o example.bc
cargo run -p capyc -- disasm example.bc

# Compile + run from source, or load + run a precompiled module.
cargo run -p capyc -- run example.cl --entry main --budget 100000
cargo run -p capyc -- run example.bc

# Interactive REPL (one expression per line; composes with pipes).
cargo run -p capyc -- repl
echo '1 + 2 * 3' | cargo run -p capyc -- repl
```

The CLI exits `0` on a clean lex, `1` when at least one diagnostic was
emitted (output is still printed) and `2` on usage / I/O errors. Combine
`--counts` with `--no-trivia` for a quick syntactic-shape summary.

`.github/workflows/ci.yml` runs `make validate`; `.github/workflows/rust.yml`
runs the Rust pipeline (fmt, clippy, target tests, doctests, anti-drift on
goldens).

## Documentation map

- `docs/lexer.md` - S1 contract: tokens, recovery, spans, dump format
- `docs/grammar.ebnf` - lexical grammar (active) + reserved syntactic section
- `docs/bytecode-v0.md` - bytecode container header frozen for CapyOS-Etapa-15
- `docs/integration.md` - CapyOS / CapyLang boundary rules
- `docs/compatibility.md` - ABI and sandbox contract
- `docs/roadmap.md` - master development plan (ordered slices, dependencies, path to Etapa 15)
- `docs/aggregates.md` - S6.2 array value-model design (implemented)
- `docs/structs-enums.md` - S6.3 struct/enum + `match` aggregate design (planned)
- `CHANGELOG.md` - release notes anchored to slice IDs

## Integration rule

CapyOS may only load CapyLang artifacts through a versioned host ABI and sandboxed bytecode loader. This repository must remain buildable and testable without CapyOS kernel headers.
