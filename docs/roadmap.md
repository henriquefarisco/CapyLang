# CapyLang development roadmap (master plan)

This is the authoritative, ordered development plan for CapyLang. It
sequences the remaining work toward the integration goal (interactive
benchmarks such as Snake / Asteroids running on CapyOS, gated by CapyOS
Etapa 15) and records, per slice, the dependencies, the acceptance
criteria and the validation gate.

- CapyLang version: `0.1.8`
- CapyOS core pinned: `0.8.0-alpha.262+20260602` (see `docs/compatibility.md`)
- Authority order for decisions: `docs/compatibility.md` -> `docs/integration.md`
  -> `docs/lexer.md` -> `docs/grammar.ebnf` -> `docs/bytecode-v0.md` ->
  this file -> `CHANGELOG.md` -> CapyOS cross-repo docs.

## Working principles

- **One slice at a time.** Each slice is anchored in `CHANGELOG.md` by a
  slice ID and is small enough to review and validate on its own.
- **Additive within v0.** New `TokenKind` / AST / opcode / error-code
  surface is added; existing surface is never renamed, reordered or
  removed inside the v0 major.
- **Fail-closed and deterministic.** Malformed input becomes a typed,
  recoverable diagnostic; the VM never panics on user input; identical
  inputs produce identical artefacts and traces.
- **Decoupled from CapyOS.** No kernel headers, no syscalls; the workspace
  builds and tests as a plain Rust workspace.
- **Validate before the next slice.** A slice is only "done" once
  `make rust-validate` (fmt + clippy `-D warnings` + workspace tests +
  doctests) and `make validate` (doc/policy gates) pass, the relevant
  docs are updated, and any ABI change is mirrored in the CapyOS
  compatibility matrix.

## Delivered (S1 - S7)

| Slice | Surface | Crate |
|---|---|---|
| S1 | lexer, dump, predicates, 23 goldens | `capy-lexer`, `capyc-tokens` |
| S2.0 - S2.3b | recursive-descent parser + span-preserving AST, 33 goldens | `capy-parser`, `capy-ast` |
| S2.2b | `match` + patterns (frontend; emitter lowers wildcard / ident / literal / range / or) | `capy-parser`, `capy-emitter` |
| S3 | structured diagnostics (severity, code, labels, SourceMap, render, bridges) | `capy-diagnostics` |
| S4 / S4b | bytecode v0 container: frozen header + typed sections | `capy-bytecode` |
| S5a / S5c | v0 opcode set + instruction codec; static stack-balance verifier | `capy-bytecode` |
| S5b / S7 | AST -> bytecode emitter; calls, params, short-circuit, `import` -> `HostCall` | `capy-emitter` |
| S6.1 | deterministic stack-machine VM, call frames, budget, `HostAdapter` | `capy-vm` |

The first complete pipeline `source -> tokens -> AST -> bytecode ->
execution` works end-to-end (`Vm::from_module(bytes).run("main")`).

## In flight (implemented, pending external validation)

These slices are implemented in the working tree but must pass
`make rust-validate` and be committed before they count as delivered:

| Slice | Surface |
|---|---|
| S2.4 | assignment / mutation (`=` and compound `+=` `-=` `*=` `/=` `%=` on a local) |
| S2.5 | `for` loops over integer ranges (`..` / `..=`), emitter-lowered |
| S5d | integer bitwise / shift operators and `~` (opcodes 0x36-0x3A, 0x51) |
| - | `Str + Str` concatenation in the VM (`Add` opcode, no wire change) |
| S12 | `capyc` CLI completed: `check` (rustc-style diagnostics) and `repl` |
| S6.2 | aggregate value model — arrays (`Value::Array`, opcodes 0x60-0x63, `V0017`), literals / indexing / indexed assignment |
| S6.2b | growable arrays — `array_push` / `array_pop` (opcodes 0x67-0x68, reference semantics, `V0019 POP_EMPTY_ARRAY`); bytecode/VM only, no frontend yet |
| S6.2c | array insert / remove — `array_insert` / `array_remove` (opcodes 0x69-0x6A, reference semantics, reuses `V0017 INDEX_OUT_OF_BOUNDS`); bytecode/VM only, no frontend yet |
| S6.3a | tagged-aggregate value model (`Value::Aggregate { tag, fields }`, opcodes `make_aggregate`/`get_field`/`get_tag` 0x64-0x66, `V0018 FIELD_OUT_OF_BOUNDS`) — bytecode/VM only, no frontend yet |
| S6.3b | enum support in the emitter — variant tag registry, unit/tuple construction (`Path` / `Call` → `MakeAggregate`), `match` `Path` / `TupleStruct` lowering (`GetTag` test + recursive `GetField`); no new opcodes |
| S6.3c | struct support — `Expr::StructLit` AST + parser (with the `no_struct_literal` head context), struct field-layout registry, literal construction (reordered to declared order), `match` `Struct` pattern lowering; no new opcodes |

## Remaining slices (ordered)

The ordering reflects dependencies: each phase unblocks the next, and the
whole sequence converges on the Snake / Asteroids benchmark goal.

### Phase A - language core completeness

- **S6.2 - aggregate value model: arrays (VM). Implemented; pending
  validation.** `Value::Array` (reference semantics via
  `Rc<RefCell<Vec<Value>>>`) plus opcodes `make_array` / `array_get` /
  `array_set` / `array_len` (0x60-0x63), the `V0017 INDEX_OUT_OF_BOUNDS`
  trap, array literals `[..]`, indexing `a[i]` and indexed assignment
  `a[i] = v`. Tuples / structs / enum payloads build on the same
  machinery and are S6.3 (below).
  - Unblocks: variable-length data (the snake body), 2D grids.
  - Design / detail: `docs/aggregates.md`.

- **S6.2b - growable arrays (VM). Implemented; pending validation.**
  Extends S6.2 with the two `Vec`-style opcodes `array_push` (0x67) and
  `array_pop` (0x68) so a bound array can actually change length, plus
  the `V0019 POP_EMPTY_ARRAY` trap for a fail-closed pop of an empty
  array. Reference semantics are preserved (push mutates the shared
  backing and returns the same handle). Bytecode + VM only — the
  `a.push(x)` / `a.pop()` frontend surface waits on method-call / stdlib
  work (S10). This is the piece that makes "variable-length data (the
  snake body)" genuinely expressible rather than fixed-capacity.
  - Depends on: S6.2.
  - Design / detail: `docs/aggregates.md`.

- **S6.2c - array insert / remove (VM). Implemented; pending validation.**
  Completes S6.2b with the two positional `Vec`-style opcodes
  `array_insert` (0x69) and `array_remove` (0x6A) so a bound array can grow
  or shrink at an arbitrary index, not only at the tail. An `idx == len`
  insert appends (matching `array_push`); an out-of-range insert / remove is
  fail-closed with the existing `V0017 INDEX_OUT_OF_BOUNDS` (no new trap).
  Reference semantics are preserved (both mutate the shared backing in
  place; insert returns the same handle). Bytecode + VM only — the
  `a.insert(i, x)` / `a.remove(i)` frontend surface waits on method-call /
  stdlib work (S10).
  - Depends on: S6.2b.
  - Design / detail: `docs/aggregates.md`.

- **S6.3 / S2.2c - struct / enum at runtime + `match` aggregate
  lowering.** Lower `Item::Struct` / `Item::Enum` and construction
  expressions onto the S6.2 aggregates; complete the `match` lowering for
  tuple-struct / struct / path patterns (today these parse but the
  emitter rejects them with `UnsupportedFeature`). Replace the
  emitter's `E0002 UnsupportedItem` for `struct` / `enum` with real
  lowering.
  - Depends on: S6.2.
  - Acceptance: construct + destructure round-trip; `match` arms bind
    fields; end-to-end tests; goldens.
  - Staging: **S6.3a (value model + opcodes + verifier + VM + `V0018`),
    S6.3b (enum construction + `match` `Path` / `TupleStruct`) and S6.3c
    (`struct` literals + `Struct` match, including the `no_struct_literal`
    parser context) — all implemented, pending external validation.**
    S6.3 is now feature-complete (construct + destructure for enums and
    structs); field-access `p.x` stays deferred to the type checker.
  - Design / detail: `docs/structs-enums.md`.

- **S2.3c - `trait` / `impl` + extended type forms** (tuple, function,
  array, reference, generic types). Lower priority for the benchmark
  goal; can land after Phase B if needed.

- **S10 - `capy-stdlib` + array methods.** A minimal standard-library subset
  callable from bytecode (length, min / max, basic numeric / string helpers),
  built on the aggregate model.
  - **First slice delivered (0.1.9):** source-level array methods
    `a.push(x)` / `a.pop()` / `a.insert(i, x)` / `a.remove(i)` / `a.get(i)` /
    `a.set(i, x)` / `a.len()` lower onto the existing `0x60-0x6A` opcodes
    (emitter-only, no new wire; new error `E0022 METHOD_ARITY`). This closes
    the `a.push(x)` frontend gap the S6.2b/S6.2c slices left open.
  - **Second slice delivered (0.1.10):** integer `min(a, b)` / `max(a, b)`
    numeric builtins lower to compare + conditional-jump (emitter-only, no new
    wire, no const-pool); user-defined `min` / `max` shadow the builtins, and
    wrong arity reuses `E0022`.
  - **Third slice delivered (Unreleased):** integer `clamp(x, lo, hi)` =
    `max(lo, min(x, hi))` lowers to two compare-and-branch selects (emitter-only,
    no new opcode, no const-pool); a user-defined `clamp` shadows it and wrong
    arity reuses `E0022`.
  - Remaining: the broader string stdlib helpers and non-integer numerics.
  - Depends on: S6.2 (collections).

### Phase B - host ABI and CapyOS integration

- **S11 - `capy-host-abi` mock crate.** In-repo, host-testable mock
  implementations of the seven ABI modules (`time`, `log`, `fs`,
  `config`, `gfx2d`, `input`, `metrics`) so programs that import them can
  be exercised deterministically without CapyOS. Extends today's
  `HostAdapter` reference stubs (`time::now`, `log::info`).
  - Depends on: S7 host-call bridge (done).

- **S8 - S9 - real host ABI surface.** Freeze the per-module function
  signatures (arguments / returns as opaque handles) for `time`, `log`,
  `fs` (sandboxed), `config`, `gfx2d`, `input`, `metrics`. The CapyLang
  side provides declarations + the mock; the *real* adapter is owned by
  CapyOS.
  - Depends on: S11, and the CapyOS-side contract.

- **CapyOS Etapa 15 (external, CapyOS-owned).** Kernel-side bytecode
  loader, sandbox policy and host-ABI adapter. CapyLang cannot open this
  gate; until it opens, CapyOS loads no CapyLang artefact. CapyLang's job
  is to keep producing artefacts that match `docs/bytecode-v0.md` and the
  integration contract exactly.

### Phase C - benchmarks

- **S13 - benchmark programs (`benchmarks/snake/`,
  `benchmarks/asteroids/`).** Per
  `CapyOS/docs/reference/integration/benchmark-harness-integration-contract.md`:
  deterministic input replay (scripted moves, not live input), stable
  metrics via `metrics`, drawing via `gfx2d`.
  - Depends on: S6.2 + S6.3 (data model), S8-S9 + S11 (host ABI),
    Etapa 15 (to actually run on CapyOS).

## Path to Snake (dependency summary)

A Snake game (even headless / replay-only) requires, at minimum:

1. mutable state and loops - **done** (S2.4 / S2.5, pending validation);
2. variable-length collections + a grid - **S6.2** (not started);
3. `struct` / `enum` at runtime for `Point` / `Direction` / state -
   **S6.3** (not started);
4. drawing and input host modules - **S8-S9 / S11** (not started);
5. a CapyOS surface that can load and run the artefact - **Etapa 15**
   (external, not open);
6. the deterministic benchmark harness - **S13**.

So Snake is the declared end goal but remains several large slices away;
the immediate critical path is the **aggregate value model (S6.2)**,
which unblocks data structures, `struct` / `enum`, richer `match`, and
ultimately the benchmark.

## Per-slice acceptance gate

Before a slice is marked done in `CHANGELOG.md`:

- `make rust-validate` is green (fmt, clippy `-D warnings`, workspace
  tests, doctests);
- `make validate` is green (doc / policy gates: no tabs in docs, VERSION
  present, README version sync, security invariants);
- new public surface has goldens or end-to-end tests;
- `docs/compatibility.md`, `docs/bytecode-v0.md`, `docs/grammar.ebnf` and
  this roadmap are updated as applicable;
- any `capy-lang-host` ABI change is mirrored in
  `CapyOS/docs/reference/integration/compatibility-matrix.md` and the
  integration contract.
