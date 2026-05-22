//! Parser diagnostics.
//!
//! Mirrors the lexer model: the parser never aborts on malformed input. It
//! emits one [`ParseDiagnostic`] per unrecoverable point, attaches an
//! [`Expr::Error`](capy_ast::Expr::Error) placeholder to the AST and keeps
//! consuming tokens until EOF.

#![forbid(unsafe_code)]

use capy_lexer::{LexErrorKind, Span, TokenKind};

/// Classification attached to a [`ParseDiagnostic`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// An unexpected token was encountered. `found` may be `None` only when
    /// the underlying lexer produced an [`TokenKind::Error`] token; in that
    /// case the paired [`ParseErrorKind::Lex`] diagnostic gives the
    /// concrete reason.
    UnexpectedToken {
        found: TokenKind,
        expected: &'static str,
    },
    /// EOF reached while a production still expected more input.
    UnexpectedEof { expected: &'static str },
    /// Surfacing of a lexer-level diagnostic produced during parsing.
    Lex(LexErrorKind),
}

/// A single recoverable parser error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub kind: ParseErrorKind,
    pub span: Span,
}

impl ParseDiagnostic {
    /// Convenience constructor.
    #[must_use]
    pub const fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}
