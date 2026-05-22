# Contributing to CapyLang

This document captures the conventions, gates and workflows used by the
CapyLang repository. It is meant for human contributors and for AI pair
programmers operating inside the workspace.

> CapyLang is the external language-core repository for CapyOS. Every
> contribution must keep this repo buildable and testable **without** the
> CapyOS kernel headers (see `docs/integration.md`).

## Quick start

1. Install the toolchain pinned by `rust-toolchain.toml` (stable channel,
   plus `rustfmt` and `clippy` components):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup show active-toolchain   # should print the pinned stable version
   ```

2. Validate the whole workspace before opening a pull request:
   ```bash
   make rust-validate    # fmt + clippy + cargo test (targets + doctests)
   make validate         # legacy doc/policy gates (no Rust required)
   ```

3. Inspect the lexer on a snippet:
   ```bash
   cargo run -p capyc-tokens -- crates/capy-lexer/tests/fixtures/lexer/10_complete.cl
   echo 'let x = 42' | cargo run -p capyc-tokens
   ```

## Repository layout

See the *Active layout* section of `README.md` for the canonical map. The
key directories for contributors are:

- `crates/capy-lexer/` - the published lexer library; the only place where
  raw source bytes are inspected.
- `crates/capy-lexer/tests/fixtures/lexer/` - golden fixtures (`.cl` and
  matching `.tokens`) anchoring the canonical dump format.
- `crates/capyc-tokens/` - thin CLI used for manual inspection and shell
  pipelines.
- `docs/` - normative documents (grammar, bytecode header, lexer contract,
  CapyOS integration boundary).

## Coding rules

### Formatting and lints

- `rustfmt` is mandatory. CI runs `cargo fmt --all -- --check`. There is no
  per-crate override.
- Clippy runs at `clippy::all = warn` workspace-wide (`Cargo.toml`).
  `RUSTFLAGS="-D warnings"` is set in CI so any warning becomes a build
  failure. Run `make clippy` locally before pushing.

### Safety

- `#![forbid(unsafe_code)]` is applied per module in every file that does
  not host a third-party `#[derive]` expansion. New modules must follow
  the same rule.
- The single file allowed to omit the forbid is `crates/capy-lexer/src/token.rs`
  (hosts `#[derive(Logos)]` which expands to lookup tables that may use
  `unsafe`). Add a comment explaining the deviation if you ever need
  another exception.

### Documentation

- Public items must carry a doc comment. The crate-level documentation is
  enabled as a doctest target (see `lib.rs`); any code sample inside `//!`
  or `///` must compile under `cargo test --doc`.
- Span-bearing examples should use placeholder offsets and never hardcode
  byte counts that may drift.

## Adding or changing tokens

When you introduce, rename or remove a `TokenKind` variant:

1. Update `crates/capy-lexer/src/token.rs` in the matching category
   section. Keep variants alphabetised inside each category whenever
   possible.
2. If the variant should print its source text in the dump (e.g. it is a
   literal or identifier-like token), extend `TokenKind::carries_text`.
3. Update `docs/grammar.ebnf` (single source of truth for the lexical
   grammar). Lexer behaviour and grammar must agree.
4. Add at least one fixture covering the new variant in isolation and one
   exercising it next to surrounding tokens. Fixtures live at
   `crates/capy-lexer/tests/fixtures/lexer/`. Naming: `NN_topic.cl` plus a
   matching `NN_topic.tokens`. Pick the lowest unused `NN`.
5. Compute the `.tokens` file by hand. Cross-check byte offsets with
   `wc -c file.cl` and `od -c file.cl` for non-ASCII content.
6. Regenerate the goldens **only** if you have already validated each
   fixture by hand:
   ```bash
   make update-goldens
   git diff crates/capy-lexer/tests/fixtures/lexer
   ```
   CI rejects accidental updates by failing if `git diff` is non-empty
   on `tests/fixtures` after `cargo test`.
7. Add a `CHANGELOG.md` entry under the `Unreleased` section. Use
   `### Added`, `### Changed`, `### Removed` per Keep a Changelog 1.1.

## Working with goldens

The dump format is part of the lexer contract (`docs/lexer.md`). The
`golden.rs` harness:

- iterates `tests/fixtures/lexer/*.cl`,
- compares the canonical dump against the matching `.tokens`,
- when `CAPY_GOLDEN_UPDATE=1` is set, rewrites the `.tokens` instead of
  asserting.

Never set `CAPY_GOLDEN_UPDATE` on CI. The Rust workflow includes an
anti-drift step that diffs `tests/fixtures` after the test phase and
fails the build on any change.

## Pull request checklist

Before requesting review:

- [ ] `make rust-validate` is green locally.
- [ ] `make validate` (doc / policy gates) is green locally.
- [ ] Every behavioural change has at least one new or updated test.
- [ ] Public API changes are documented under `### Added` / `### Changed`
      in `CHANGELOG.md`.
- [ ] Versioning follows Semantic Versioning. Increment the appropriate
      digit in `VERSION`, `Cargo.toml` (workspace.package.version), and
      `README.md` (`Version:` line). Major bumps require a migration note
      in `docs/lexer.md` or the corresponding contract document.
- [ ] If the change crosses the CapyOS / CapyLang boundary, link the
      affected sections of `docs/integration.md` or
      `docs/compatibility.md` in the PR description.

## Slice-driven roadmap

Releases follow the slice plan kept in `README.md`. Slice IDs (`S1`,
`S2`, ...) appear in every `CHANGELOG.md` entry and pull request title so
the history can be replayed in roadmap order. Do not merge work from a
later slice ahead of its prerequisites.

| Slice | Scope | Status |
|---|---|---|
| S1 | `capy-lexer` + golden tests + `capyc-tokens` CLI | Stable, version 0.1.1 |
| S2 | `capy-ast` + recursive-descent parser | Not started |
| S3 | Structured diagnostics + `capyc check` | Not started |
| S4 | `capy-bytecode` container (header frozen by integration contract) | Not started |

## CapyOS integration boundary

The integration contract owned by CapyOS is normative. Before opening a
pull request that touches public types, ABI surface, or bytecode layout:

- read `docs/integration.md` and `docs/compatibility.md`;
- confirm the change is consistent with
  `CapyOS/docs/reference/integration/capylang-integration-contract.md`
  (referenced in `README.md`);
- avoid any code path that calls CapyOS syscalls or dereferences CapyOS
  kernel pointers; the only legitimate transport is the versioned host
  ABI declared in the contract.

## License

CapyLang is MIT-licensed. By submitting a pull request you agree that
your contribution is published under the same terms.
