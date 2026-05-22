//! Core diagnostic types.
//!
//! Stability: [`Severity`], [`Code`], [`Label`] and [`Diagnostic`] are part
//! of the S3 ABI. The catalogue of stable [`Code`] constants below is
//! additive within `capy-lang-host` v0; renaming or removing a code is a
//! breaking change. New codes follow the `L<NNNN>` (lexer), `P<NNNN>`
//! (parser), `B<NNNN>` (bytecode), `V<NNNN>` (VM) and `H<NNNN>` (host ABI)
//! prefix conventions.

#![forbid(unsafe_code)]

use capy_lexer::Span;

/// Severity of a diagnostic. Frozen in S3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Fatal for the affected unit (script, module, file).
    Error,
    /// Non-fatal but visible to the user.
    Warning,
    /// Auxiliary context for another diagnostic.
    Note,
    /// Suggested fix or hint.
    Help,
}

impl Severity {
    /// Lower-case mnemonic used by [`crate::render`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Help => "help",
        }
    }
}

/// Stable, human-readable diagnostic code.
///
/// Codes are `&'static str` so they can be used in `const` items, matched
/// directly and embedded in published error documentation. The exact set
/// of codes is part of the `capy-lang-host` v0 contract — see the module
/// docs for the prefix scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code(pub &'static str);

impl Code {
    /// Returns the code as a `&'static str`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// A labelled span attached to a diagnostic.
///
/// The `message`, when present, is displayed inline after the caret in the
/// rendered output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: Option<String>,
}

impl Label {
    /// Builds a label with no message.
    #[must_use]
    pub const fn new(span: Span) -> Self {
        Self {
            span,
            message: None,
        }
    }

    /// Builds a label with an inline message.
    #[must_use]
    pub fn with_message(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
        }
    }
}

/// A fully-structured diagnostic.
///
/// A diagnostic always has exactly one primary span and at most a list of
/// secondary spans (for cross-references such as "previous definition was
/// here"). `notes` are free-form trailing strings rendered with a `= note:`
/// prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Builds a fresh [`Severity::Error`] diagnostic with `code`, top-level
    /// `message` and a primary span.
    #[must_use]
    pub fn error(code: Code, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            primary: Label::new(span),
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Replaces the primary label's inline message.
    #[must_use]
    pub fn with_label_message(mut self, message: impl Into<String>) -> Self {
        self.primary.message = Some(message.into());
        self
    }

    /// Appends a secondary label.
    #[must_use]
    pub fn with_secondary(mut self, label: Label) -> Self {
        self.secondary.push(label);
        self
    }

    /// Appends a trailing note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

// === Stable code catalogue ==============================================
//
// Adding a code is additive within `capy-lang-host` v0. Renaming or
// removing one is a breaking change and must be reflected in
// `docs/compatibility.md` (error model row) and CapyOS's
// compatibility-matrix on the same commit.

/// Lexer: unterminated `"` string literal.
pub const L_UNTERMINATED_STRING: Code = Code("L0001");
/// Lexer: unterminated `/* ... */` block comment.
pub const L_UNTERMINATED_BLOCK_COMMENT: Code = Code("L0002");
/// Lexer: input byte sequence that does not match any token.
pub const L_UNKNOWN_CHAR: Code = Code("L0003");
/// Parser: unexpected token (with expected category in the label).
pub const P_UNEXPECTED_TOKEN: Code = Code("P0001");
/// Parser: end of input reached while a production required more.
pub const P_UNEXPECTED_EOF: Code = Code("P0002");
