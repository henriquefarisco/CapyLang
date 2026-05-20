# CapyLang

Version: 0.1.3

CapyLang is the external language-core repository for CapyOS.

## Status

Slice **S1 (lexer)** is implemented and polished under `crates/capy-lexer`,
with a thin `capyc-tokens` debug CLI living in `crates/capyc-tokens`. The
lexer is span-tracking, produces recoverable diagnostics, ships 23 golden
fixtures and exposes two stable `TokenKind` predicates (`carries_text`,
`is_trivia`). Parser, bytecode and VM slices follow the roadmap in
`.windsurf/plans/capylang-roadmap-0c1ca5.md`.

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

CapyOS core pinned: `0.8.0-alpha.241+20260519`.

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
  capyc-tokens/           # S1 - debug CLI (dump / counts / no-trivia modes)
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
  capy-lexer/      # S1 (done)
  capy-parser/     # S2
  capy-bytecode/   # S4
  capy-vm/         # S6-S9
  capy-stdlib/     # S10
  capy-host-abi/   # S11
  capyc/           # S12 CLI (run, compile, disasm, repl)
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
