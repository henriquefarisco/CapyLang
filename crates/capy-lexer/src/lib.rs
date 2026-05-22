//! CapyLang lexer (slice S1).
//!
//! Produces a deterministic stream of [`Token`] values with byte-level
//! [`Span`]s plus a list of recoverable [`Diagnostic`]s for malformed input.
//!
//! The lexer is intentionally lossless: whitespace, newlines and comments
//! are emitted as their own token kinds so that downstream tooling
//! (formatter, syntax highlighter, parser with trivia preservation) can
//! reconstruct the source byte-for-byte.
//!
//! # Example
//!
//! ```
//! use capy_lexer::{tokenize, TokenKind};
//!
//! let result = tokenize("let x = 42");
//! assert!(result.diagnostics.is_empty());
//! assert_eq!(result.tokens.first().map(|t| t.kind), Some(TokenKind::Let));
//! ```

// Allow unsafe only inside the module that hosts the `#[derive(Logos)]` output.
// Every other module sets `#![forbid(unsafe_code)]` explicitly below.

mod diagnostic;
mod dump;
mod lexer;
mod token;

pub use diagnostic::{Diagnostic, LexErrorKind};
pub use dump::dump_tokens;
pub use lexer::{tokenize, LexResult, Lexer, Span, Token};
pub use token::TokenKind;
