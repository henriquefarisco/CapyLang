# CapyLang

Version: 0.1.6

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
error catalogue `V0001`-`V0013` (S5b.3 added `V0011`
`CALL_STACK_OVERFLOW`, `V0012` `UNKNOWN_FUNCTION_INDEX` and `V0013`
`CALL_ARITY_MISMATCH`). No JIT, no syscalls, no host pointers, no
global state.

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

CapyOS core pinned: `0.8.0-alpha.260+20260525`.

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
      opcode.rs           # Opcode enum (24 frozen byte values) + Imm shape
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
      value.rs            # Value (None/Bool/Int/Float/Str)
      execute.rs          # Vm loader + interpreter loop
      error.rs            # VmError + V0001..V0016 codes
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
  capyc/                  # S12 - unified CLI (tokens/parse/compile/disasm/run)
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
  capyc/           # S12 unified CLI (tokens/parse/compile/disasm/run) (sketch done)
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

# Compile to bytecode and disassemble it back.
cargo run -p capyc -- compile example.cl -o example.bc
cargo run -p capyc -- disasm example.bc

# Compile + run from source, or load + run a precompiled module.
cargo run -p capyc -- run example.cl --entry main --budget 100000
cargo run -p capyc -- run example.bc
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
- `CHANGELOG.md` - release notes anchored to slice IDs

## Integration rule

CapyOS may only load CapyLang artifacts through a versioned host ABI and sandboxed bytecode loader. This repository must remain buildable and testable without CapyOS kernel headers.
