//! Recoverable lexer diagnostics.
//!
//! The lexer never aborts: it always produces a complete token stream and
//! attaches diagnostics for every byte-range it could not classify. This
//! mirrors the contract documented in `docs/integration.md` ("deterministic
//! VM errors") and lets the parser and CLI surface multiple errors per run.

#![forbid(unsafe_code)]

use crate::lexer::Span;

/// Classification attached to a [`crate::TokenKind::Error`] token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LexErrorKind {
    /// A string literal was opened with `"` but no matching close was found
    /// before EOF or before a raw (unescaped) newline.
    UnterminatedString,
    /// A `/*` block comment was opened but no matching `*/` was found,
    /// honouring nesting, before EOF.
    UnterminatedBlockComment,
    /// A byte sequence that does not match any other token kind.
    UnknownChar,
}

/// A single recoverable lexer error.
///
/// Diagnostics are emitted in source order; the span is the byte range of
/// the offending text. The lexer guarantees `span.start <= span.end` and that
/// both endpoints fall on UTF-8 character boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: LexErrorKind,
    pub span: Span,
}

impl Diagnostic {
    /// Convenience constructor.
    #[must_use]
    pub const fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}
