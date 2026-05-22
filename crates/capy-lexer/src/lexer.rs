//! `logos`-backed lexer wrapper.
//!
//! Wraps the generated state machine with:
//!
//! * a typed [`Span`] (instead of `Range<usize>`);
//! * a single owning [`Token`] struct;
//! * error recovery via [`crate::Diagnostic`]: every malformed range becomes
//!   a [`crate::TokenKind::Error`] token plus a classified diagnostic;
//! * an explicit terminal [`crate::TokenKind::Eof`] token so consumers can
//!   uniformly drive their state machine without checking for `None`.

#![forbid(unsafe_code)]

use logos::Logos;

use crate::diagnostic::{Diagnostic, LexErrorKind};
use crate::token::TokenKind;

/// Inclusive-start, exclusive-end byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Constructs a span from raw byte offsets.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    /// `true` when the span covers no bytes (e.g. the synthetic EOF marker).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl From<std::ops::Range<usize>> for Span {
    fn from(r: std::ops::Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}

/// A lexer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Output of [`tokenize`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Convenience wrapper around [`tokenize`] that yields tokens lazily.
///
/// Intended for tooling that consumes the token stream incrementally (REPL,
/// editor highlighter). For batch use prefer [`tokenize`] which returns a
/// fully-materialised [`LexResult`].
#[derive(Debug)]
pub struct Lexer<'src> {
    source: &'src str,
    inner: logos::Lexer<'src, TokenKind>,
    emitted_eof: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a new lexer over `source`.
    #[must_use]
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            inner: TokenKind::lexer(source),
            emitted_eof: false,
        }
    }

    /// Returns the original source byte slice.
    #[must_use]
    pub const fn source(&self) -> &'src str {
        self.source
    }

    /// Returns the next token paired with an optional diagnostic.
    ///
    /// The lexer guarantees:
    ///
    /// * at least one token is produced (the synthetic [`TokenKind::Eof`]);
    /// * spans are non-overlapping and cover the source byte-for-byte;
    /// * every [`TokenKind::Error`] token is paired with a [`Diagnostic`].
    pub fn next_token(&mut self) -> Option<(Token, Option<Diagnostic>)> {
        match self.inner.next() {
            Some(result) => {
                let span: Span = self.inner.span().into();
                match result {
                    Ok(kind) => Some((Token { kind, span }, None)),
                    Err(()) => {
                        // Do NOT use `self.inner.slice()`: logos returns it via
                        // `from_utf8_unchecked` and the span may fall in the middle
                        // of a multi-byte UTF-8 sequence when logos advances byte
                        // by byte over unmatched input. `str::get` is bounds and
                        // boundary checked, falling back to an empty slice keeps
                        // the classifier safe in that worst case.
                        let slice = slice_safe(self.source, span);
                        let kind = classify_error(slice);
                        let diag = Diagnostic::new(kind, span);
                        Some((
                            Token {
                                kind: TokenKind::Error,
                                span,
                            },
                            Some(diag),
                        ))
                    }
                }
            }
            None => {
                if self.emitted_eof {
                    None
                } else {
                    self.emitted_eof = true;
                    let end = self.source.len();
                    Some((
                        Token {
                            kind: TokenKind::Eof,
                            span: Span::new(end, end),
                        },
                        None,
                    ))
                }
            }
        }
    }
}

impl<'src> Iterator for Lexer<'src> {
    type Item = (Token, Option<Diagnostic>);

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

/// Eagerly tokenises `source` into a [`LexResult`].
///
/// Always produces a trailing [`TokenKind::Eof`] token even for empty input.
#[must_use]
pub fn tokenize(source: &str) -> LexResult {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    while let Some((token, diag)) = lexer.next_token() {
        tokens.push(token);
        if let Some(d) = diag {
            diagnostics.push(d);
        }
    }
    LexResult {
        tokens,
        diagnostics,
    }
}

/// Classifies an unrecognised lexer slice into a concrete [`LexErrorKind`].
fn classify_error(slice: &str) -> LexErrorKind {
    if slice.starts_with('"') {
        LexErrorKind::UnterminatedString
    } else if slice.starts_with("/*") {
        LexErrorKind::UnterminatedBlockComment
    } else {
        LexErrorKind::UnknownChar
    }
}

/// Returns the substring of `source` covered by `span`, or an empty string
/// when the span lies in the middle of a multi-byte UTF-8 sequence or
/// otherwise outside the source. Never panics.
pub(crate) fn slice_safe(source: &str, span: Span) -> &str {
    source.get(span.start..span.end).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::tokenize;
    use crate::diagnostic::LexErrorKind;
    use crate::token::TokenKind;

    #[test]
    fn empty_source_emits_only_eof() {
        let result = tokenize("");
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(result.tokens[0].kind, TokenKind::Eof);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn spans_cover_source_byte_for_byte() {
        let source = "let x = 42";
        let result = tokenize(source);
        // Drop the trailing Eof for the coverage check.
        let body = &result.tokens[..result.tokens.len() - 1];
        let mut cursor = 0;
        for tok in body {
            assert_eq!(tok.span.start, cursor, "gap at {cursor}");
            cursor = tok.span.end;
        }
        assert_eq!(cursor, source.len());
    }

    #[test]
    fn keywords_outprioritise_idents() {
        let result = tokenize("fn fnord");
        let kinds: Vec<_> = result.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Fn,
                TokenKind::Whitespace,
                TokenKind::Ident,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn unterminated_string_is_recoverable() {
        let result = tokenize("\"oops");
        assert_eq!(result.tokens[0].kind, TokenKind::Error);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].kind, LexErrorKind::UnterminatedString);
        // Lexer kept going and produced Eof.
        assert_eq!(result.tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn nested_block_comments() {
        let result = tokenize("/* a /* b */ c */");
        let kinds: Vec<_> = result.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, vec![TokenKind::BlockComment, TokenKind::Eof]);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn float_outprioritises_int() {
        let result = tokenize("3.14");
        assert_eq!(result.tokens[0].kind, TokenKind::Float);
    }

    #[test]
    fn unmatched_multibyte_does_not_panic() {
        // `€` is U+20AC encoded as three bytes 0xE2 0x82 0xAC. It is not in
        // `\p{XID_Start}` so the Ident regex rejects it. Logos's default
        // recovery advances one byte at a time, producing spans that fall in
        // the middle of the UTF-8 sequence. The wrapper must classify those
        // bytes without panicking on a malformed slice.
        let result = tokenize("\u{20AC}");
        assert!(!result.diagnostics.is_empty());
        assert!(result
            .diagnostics
            .iter()
            .all(|d| d.kind == LexErrorKind::UnknownChar));
        assert_eq!(result.tokens.last().unwrap().kind, TokenKind::Eof);
    }
}
