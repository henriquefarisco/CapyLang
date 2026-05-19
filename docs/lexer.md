# CapyLang Lexer (S1)

Status: **stable**, version `0.1.1`.

This document describes the deterministic contract between the Rust crate
`capy-lexer` and every downstream consumer (parser, formatter, IDE tooling,
`capyc-tokens` debug CLI). It complements two artefacts:

- `docs/grammar.ebnf` - the canonical lexical grammar in EBNF;
- `crates/capy-lexer/tests/fixtures/lexer/` - 23 byte-precise golden tests
  that anchor the implementation.

The lexer is the only component allowed to inspect raw source bytes. Every
subsequent stage operates on the token stream described here.

## Design goals

1. **Lossless.** Whitespace, newlines, line comments and block comments are
   emitted as their own token kinds. Concatenating the spans of all tokens
   reconstructs the source byte-for-byte.
2. **Recoverable.** The lexer never aborts. Malformed regions become
   `Token::Error` tokens paired with a typed `Diagnostic`. Lexing always
   reaches the synthetic terminal `Token::Eof` at offset `[len, len)`.
3. **Deterministic.** Output depends only on the input bytes. No locale,
   environment variables or wall-clock state participates in any decision.
4. **Span-precise.** Every span is a half-open `[start, end)` range of byte
   offsets. Endpoints fall on UTF-8 character boundaries for tokens emitted
   by valid rules; the safe fallback `slice_safe` returns an empty string
   for the rare case where logos recovers byte-by-byte through invalid
   input.

## Token categories

The complete enum lives at `crates/capy-lexer/src/token.rs::TokenKind` and
is grouped as follows. Adding a new variant is **additive** between minor
versions; renames and removals require a major bump.

### Trivia

| Variant | Pattern | Notes |
|---|---|---|
| `Whitespace` | `[ \t]+` | Greedy, never crosses a newline. |
| `Newline` | `\r?\n` | One token per line break, CRLF normalised at the token boundary, not the byte stream. |
| `LineComment` | `//[^\r\n]*` | Excludes the terminator. |
| `BlockComment` | `/* ... */` | Arbitrarily nested, see *Block comments*. |

### Literals

| Variant | Pattern | Examples |
|---|---|---|
| `Int` (decimal) | `0\|[1-9][0-9_]*` | `0`, `42`, `1_000_000` |
| `Int` (hex) | `0x[0-9a-fA-F][0-9a-fA-F_]*` | `0x1A`, `0xCAFE_BABE` |
| `Int` (binary) | `0b[01][01_]*` | `0b1010`, `0b_0000_1111` (note: leading underscore is rejected; use `0b1_0000`) |
| `Int` (octal) | `0o[0-7][0-7_]*` | `0o77`, `0o755` |
| `Float` | `[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?` | `3.14`, `1_000.5` |
| `Float` (exp only) | `[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*` | `1e10`, `2e-3` |
| `Str` | `"..."` with `\\`-escapes | `"hello"`, `"a\"b"`, `"line\\nfeed"` |

Tie-break with `Ident`: float wins over int wins over ident by priority.

### Keywords

`fn`, `let`, `const`, `if`, `else`, `while`, `for`, `in`, `loop`, `break`,
`continue`, `return`, `match`, `try`, `catch`, `throw`, `async`, `await`,
`import`, `from`, `as`, `enum`, `trait`, `impl`, `struct`, `type`, `arena`,
`pub`, `true`, `false`, `None` (`NoneKw`), `Some` (`SomeKw`).

Keywords are declared as `#[token]` literals with implicit priority equal to
their byte length. Equal-length ties with `Ident` are resolved by source
order; the keyword variants are declared **before** the `Ident` regex, so
they always win.

### Identifiers

```
Ident = ( XID_Start | "_" ) ( XID_Continue )*
```

Unicode-aware: `caf\u{00E9}` (`café`) is a single `Ident` spanning five
bytes. The implementation relies on `regex-syntax`'s `unicode-bool` feature,
enabled by default through logos 0.14's dependency.

### Punctuation

`(` `)` `{` `}` `[` `]` `,` `;` `::` `:` `...` `..` `.` `->` `=>` `?` `@`

### Operators

| Two-char | One-char | Notes |
|---|---|---|
| `==` `!=` `<=` `>=` `<<` `>>` `&&` `\|\|` | `=` `!` `<` `>` `&` `\|` `^` `~` | |
| `+=` `-=` `*=` `/=` `%=` | `+` `-` `*` `/` `%` | Compound assignment out-prioritises the single char by length. |

### Synthetic

| Variant | When emitted |
|---|---|
| `Eof` | Always, exactly once, at offset `[len, len)`. |
| `Error` | For every byte range the lexer could not classify. Paired with a `Diagnostic`. |

## Error recovery

The recovery contract is the same as the one published in
`docs/integration.md`:

> Deterministic VM errors. The runtime never panics on user input.

Three classes of `LexErrorKind` are produced:

| Kind | Source pattern | Recovery |
|---|---|---|
| `UnterminatedString` | `"...` reaches EOF or a raw newline. | Emit `Token::Error` spanning the literal up to the newline / EOF. Lexing resumes at the newline. |
| `UnterminatedBlockComment` | `/* ...` reaches EOF with non-zero nesting depth. | Emit `Token::Error` spanning the rest of the source. Lexing resumes at EOF (no further input). |
| `UnknownChar` | Any byte the master DFA cannot start a token with. | Emit `Token::Error` spanning the offending byte. Lexing resumes at the next byte. |

The lexer **never** drops bytes silently: spans concatenate to cover the
entire source, including bytes inside error tokens.

## Spans

```rust
pub struct Span {
    pub start: usize,
    pub end:   usize, // exclusive
}
```

Guarantees:

- `0 <= start <= end <= source.len()`;
- non-overlapping across the stream;
- concatenated coverage of the source for the body of the stream
  (everything except the trailing `Eof`);
- the `Eof` token has `start == end == source.len()`.

Slicing the source by a token's span is safe **for tokens emitted by valid
rules**. For `Token::Error` slicing through `&source[span]` may panic if
logos recovered byte-by-byte over invalid UTF-8. Use the `slice_safe`
helper (or `source.get(start..end)`) when consuming Error spans.

## Block comments

```
"/*"      depth := 1
"/*"      depth += 1   (any time, including inside another /* ... */)
"*/"      depth -= 1
EOF       if depth > 0 => Error::UnterminatedBlockComment
```

Implemented by the `lex_block_comment` callback in
`crates/capy-lexer/src/token.rs`. Tested by `nested_block_comments` (unit
test) and `07_comments.cl`, `17_block_comment_unterm.cl` (goldens).

## String literals

Pseudo-grammar (one backslash means a literal backslash byte):

- `"` opening delimiter;
- body: any sequence of either a backslash-escape (one backslash followed
  by exactly one other byte, both consumed verbatim) or a plain byte that
  is neither `"`, `\`, nor `\n`;
- `"` closing delimiter.

The escape handling is intentionally **syntactic only**: backslash plus any
character (in source bytes) is consumed verbatim; the body character is
not inspected. A raw newline aborts the literal and produces
`UnterminatedString`. Semantic interpretation of escape sequences
(`\n`, `\t`, `\u{...}`, ...) is deferred to a later slice when string
values are needed by the AST.

## Canonical dump format

The textual form produced by `dump_tokens` and `capyc-tokens` is part of the
public contract. It powers golden tests and CLI debugging.

```
[<start>..<end>] <Kind>
[<start>..<end>] <Kind> "<text>"
[<start>..<end>] Error <ErrorKind> "<text>"
```

Stability rules:

- The fields, separators and newline policy are frozen for the v0 stream.
- New optional trailers may be appended within a minor version (`additive`).
- Changes that rename, reorder or remove fields require a major bump and a
  migration note in this document.

The `<text>` payload uses Rust's `{:?}` Debug formatter for `&str`:
embedded quotes are escaped to `\"`, literal backslashes are escaped to
`\\`, newlines to `\n`, tabs to `\t`, and so on. Tokens whose
`carries_text()` returns `true` (`Ident`, `Int`, `Float`, `Str`) include
the payload; all others omit it.

## Token predicates

Two stable predicates classify `TokenKind` values for downstream tooling:

- `TokenKind::carries_text()` returns `true` for kinds that include their
  source text in the canonical dump (`Ident`, `Int`, `Float`, `Str`).
- `TokenKind::is_trivia()` returns `true` for kinds the parser is free to
  skip (`Whitespace`, `Newline`, `LineComment`, `BlockComment`).

The two sets are disjoint. Adding or removing a kind from either set is a
breaking change and must be paired with a major version bump.

## Extending the lexer

When you add a new variant, perform the following in order:

1. Append the variant to `TokenKind` with a `#[token]` or `#[regex]`
   attribute. Place it in the matching category section so the file remains
   easy to scan.
2. If the new variant should appear in the dump payload, extend
   `TokenKind::carries_text`.
3. Add at least one golden fixture under
   `crates/capy-lexer/tests/fixtures/lexer/` covering the new variant in
   isolation and one in context with surrounding tokens.
4. Update `docs/grammar.ebnf` so the EBNF stays the single source of truth.
5. If the variant is part of a wider feature, add a `CHANGELOG.md` entry.

When you change the dump format:

1. Decide whether the change is additive (minor) or breaking (major).
2. Update the *Canonical dump format* section in this file with the new
   trailer / removed field.
3. Bump the workspace version in `Cargo.toml` and `VERSION` as appropriate.
4. Regenerate the goldens via `make update-goldens` and **review the diff
   manually** before committing.

## Validation

Running locally:

```bash
make rust-validate          # fmt + clippy + tests + doctests
cargo run -p capyc-tokens -- path/to/source.cl
cargo run -p capyc-tokens -- --counts path/to/source.cl
cargo run -p capyc-tokens -- --no-trivia path/to/source.cl
```

`--counts` emits a deterministic `<count> <Kind>` histogram (count desc,
ties broken by kind name asc). `--no-trivia` filters trivia from the
output of both modes; `Eof` is always retained and diagnostics are
preserved so the exit code stays meaningful.

Running on CI: `.github/workflows/rust.yml` performs the same steps on
every push and pull request, plus a `git diff` check that rejects
accidental golden updates.
