# Changelog

All notable changes to CapyLang are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Slice identifiers (e.g. **S1**, **S2**) refer to the roadmap captured in
`README.md` and trace each release back to a CapyOS-Etapa-15 integration gate.

## [Unreleased]

### Planned

- **S2**: hand-written recursive-descent parser, `capy-ast` crate with
  span-preserving nodes, golden AST tests.
- **S3**: structured diagnostics (label, severity, code, rendered output) and
  the `capyc check` front-end.

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

[Unreleased]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/henriquefarisco/CapyLang/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/henriquefarisco/CapyLang/releases/tag/v0.1.0
