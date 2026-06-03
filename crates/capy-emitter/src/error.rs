//! Emitter diagnostics.
//!
//! Codes are part of the `capy-lang-host` v0 ABI. Adding a code is
//! additive within v0; renaming or removing one is a breaking change
//! and must be reflected in the cross-repo compatibility matrix.

#![forbid(unsafe_code)]

use std::fmt;

use capy_lexer::Span;

/// Unsupported expression form (Path, Index, Field, Call, Match, ...).
pub const E_UNSUPPORTED_EXPR: &str = "E0001";
/// Unsupported top-level item (Const, Struct, Enum, TypeAlias, ...).
pub const E_UNSUPPORTED_ITEM: &str = "E0002";
/// Identifier reference does not resolve to a local in scope.
pub const E_UNKNOWN_LOCAL: &str = "E0003";
/// Two `let` bindings in the same function reuse the same name.
pub const E_DUPLICATE_LOCAL: &str = "E0004";
/// Integer literal text could not be parsed as `i64`.
pub const E_INTEGER_PARSE: &str = "E0005";
/// Float literal text could not be parsed as `f64`.
pub const E_FLOAT_PARSE: &str = "E0006";
/// String literal could not be decoded (malformed quotes or escape).
pub const E_STRING_PARSE: &str = "E0007";
/// Binary operator not yet supported by the emitter (e.g. `&&` / `||`
/// short-circuit lowering pending in S5b.2, bitwise ops in S5b.3).
pub const E_UNSUPPORTED_BINARY: &str = "E0008";
/// Unary operator not yet supported by the emitter (e.g. `~`).
pub const E_UNSUPPORTED_UNARY: &str = "E0009";
/// A surface feature (function parameter, `break <value>`, ...) not
/// yet implemented by the current slice.
pub const E_UNSUPPORTED_FEATURE: &str = "E0010";
/// Encountered `Expr::Error` produced by the parser's recovery path.
pub const E_PARSE_ERROR_IN_EXPR: &str = "E0011";
/// A top-level statement that is not an `Item` (e.g. a free expression
/// statement at the top level of the source).
pub const E_TOP_LEVEL_MUST_BE_ITEM: &str = "E0012";
/// An `Item` declared inside a block body (currently only top-level
/// items are emitted).
pub const E_NESTED_ITEM: &str = "E0013";
/// `break` used outside any enclosing `while` / `loop` body.
pub const E_BREAK_OUTSIDE_LOOP: &str = "E0014";
/// `continue` used outside any enclosing `while` / `loop` body.
pub const E_CONTINUE_OUTSIDE_LOOP: &str = "E0015";
/// Function call references a name that does not resolve to a
/// top-level `fn` declared in the current module.
pub const E_UNKNOWN_FUNCTION: &str = "E0016";
/// Function call callee is not a simple identifier (e.g. a path, a
/// field access, a parenthesised expression). S5b.3 supports only
/// direct calls to top-level functions.
pub const E_UNSUPPORTED_CALLEE: &str = "E0017";
/// Two top-level `fn` items declared with the same name in the same
/// module.
pub const E_DUPLICATE_FUNCTION: &str = "E0018";
/// Function declares more than `u32::MAX` parameters (currently the
/// only practical limit beyond ordinary locals).
pub const E_TOO_MANY_ARGUMENTS: &str = "E0019";
/// Two `import` items in the same module resolve to the same callable
/// name (either the same trailing path segment, or the same `as`
/// alias). The first declaration keeps the slot; the second is
/// reported and ignored. Introduced by S7 when the emitter started
/// lowering source-level calls into `HostCall`.
pub const E_DUPLICATE_IMPORT: &str = "E0020";
/// Assignment target is not an assignable place. v0 (S2.4) allows only a
/// simple local identifier on the left of `=`; field, index, call, path
/// and parenthesised targets are rejected here.
pub const E_INVALID_ASSIGN_TARGET: &str = "E0021";

/// Classification of a single emitter failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitErrorKind {
    UnsupportedExpr { what: &'static str },
    UnsupportedItem { what: &'static str },
    UnknownLocal { name: String },
    DuplicateLocal { name: String },
    IntegerParse { text: String },
    FloatParse { text: String },
    StringParse { reason: &'static str },
    UnsupportedBinary { op: &'static str },
    UnsupportedUnary { op: &'static str },
    UnsupportedFeature { what: &'static str },
    ParseErrorInExpr,
    TopLevelMustBeItem,
    NestedItem,
    BreakOutsideLoop,
    ContinueOutsideLoop,
    UnknownFunction { name: String },
    UnsupportedCallee { what: &'static str },
    DuplicateFunction { name: String },
    TooManyArguments { count: usize },
    DuplicateImport { name: String },
    InvalidAssignTarget { what: &'static str },
}

/// A single recoverable emitter error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitError {
    pub kind: EmitErrorKind,
    pub span: Span,
}

impl EmitError {
    /// Builds an error.
    #[must_use]
    pub const fn new(kind: EmitErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.kind {
            EmitErrorKind::UnsupportedExpr { .. } => E_UNSUPPORTED_EXPR,
            EmitErrorKind::UnsupportedItem { .. } => E_UNSUPPORTED_ITEM,
            EmitErrorKind::UnknownLocal { .. } => E_UNKNOWN_LOCAL,
            EmitErrorKind::DuplicateLocal { .. } => E_DUPLICATE_LOCAL,
            EmitErrorKind::IntegerParse { .. } => E_INTEGER_PARSE,
            EmitErrorKind::FloatParse { .. } => E_FLOAT_PARSE,
            EmitErrorKind::StringParse { .. } => E_STRING_PARSE,
            EmitErrorKind::UnsupportedBinary { .. } => E_UNSUPPORTED_BINARY,
            EmitErrorKind::UnsupportedUnary { .. } => E_UNSUPPORTED_UNARY,
            EmitErrorKind::UnsupportedFeature { .. } => E_UNSUPPORTED_FEATURE,
            EmitErrorKind::ParseErrorInExpr => E_PARSE_ERROR_IN_EXPR,
            EmitErrorKind::TopLevelMustBeItem => E_TOP_LEVEL_MUST_BE_ITEM,
            EmitErrorKind::NestedItem => E_NESTED_ITEM,
            EmitErrorKind::BreakOutsideLoop => E_BREAK_OUTSIDE_LOOP,
            EmitErrorKind::ContinueOutsideLoop => E_CONTINUE_OUTSIDE_LOOP,
            EmitErrorKind::UnknownFunction { .. } => E_UNKNOWN_FUNCTION,
            EmitErrorKind::UnsupportedCallee { .. } => E_UNSUPPORTED_CALLEE,
            EmitErrorKind::DuplicateFunction { .. } => E_DUPLICATE_FUNCTION,
            EmitErrorKind::TooManyArguments { .. } => E_TOO_MANY_ARGUMENTS,
            EmitErrorKind::DuplicateImport { .. } => E_DUPLICATE_IMPORT,
            EmitErrorKind::InvalidAssignTarget { .. } => E_INVALID_ASSIGN_TARGET,
        }
    }

    /// Returns the human-readable message for this error, without the
    /// stable code prefix.
    ///
    /// Use this when assembling a structured diagnostic (e.g.
    /// [`capy-diagnostics::bridge::from_emit`]) so the code is rendered
    /// once by the surrounding diagnostic layer. The CLI and other
    /// plain-text consumers should use [`fmt::Display`], which prepends
    /// the `[E<NNNN>]` code so a code-less log line still pins the
    /// diagnostic.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.kind {
            EmitErrorKind::UnsupportedExpr { what } => {
                format!("unsupported expression form: {what}")
            }
            EmitErrorKind::UnsupportedItem { what } => {
                format!("unsupported item form: {what}")
            }
            EmitErrorKind::UnknownLocal { name } => format!("unknown local `{name}`"),
            EmitErrorKind::DuplicateLocal { name } => {
                format!("duplicate local `{name}` in the same scope")
            }
            EmitErrorKind::IntegerParse { text } => {
                format!("integer literal `{text}` does not fit in i64")
            }
            EmitErrorKind::FloatParse { text } => {
                format!("float literal `{text}` cannot be parsed as f64")
            }
            EmitErrorKind::StringParse { reason } => {
                format!("string literal: {reason}")
            }
            EmitErrorKind::UnsupportedBinary { op } => {
                format!("binary operator `{op}` is not lowered yet")
            }
            EmitErrorKind::UnsupportedUnary { op } => {
                format!("unary operator `{op}` is not lowered yet")
            }
            EmitErrorKind::UnsupportedFeature { what } => {
                format!("unsupported language feature: {what}")
            }
            EmitErrorKind::ParseErrorInExpr => {
                "expression contains a parser-error placeholder".to_string()
            }
            EmitErrorKind::TopLevelMustBeItem => {
                "top-level statement must be a fn/import/struct/const/type/enum item".to_string()
            }
            EmitErrorKind::NestedItem => {
                "items can only appear at the top level of a source unit".to_string()
            }
            EmitErrorKind::BreakOutsideLoop => {
                "`break` used outside of a `while` or `loop` body".to_string()
            }
            EmitErrorKind::ContinueOutsideLoop => {
                "`continue` used outside of a `while` or `loop` body".to_string()
            }
            EmitErrorKind::UnknownFunction { name } => {
                format!("call to unknown function `{name}`")
            }
            EmitErrorKind::UnsupportedCallee { what } => {
                format!("unsupported call site: {what}")
            }
            EmitErrorKind::DuplicateFunction { name } => {
                format!("two top-level functions declared with the same name `{name}`")
            }
            EmitErrorKind::TooManyArguments { count } => {
                format!("call passes {count} arguments, exceeds the v0 limit")
            }
            EmitErrorKind::DuplicateImport { name } => {
                format!("two `import` items resolve to the same callable name `{name}`")
            }
            EmitErrorKind::InvalidAssignTarget { what } => {
                format!("cannot assign to {what}; only a local variable is assignable")
            }
        }
    }
}

/// Human-readable rendering for [`EmitError`].
///
/// Output is deterministic and starts with the stable `[E<NNNN>]`
/// code in square brackets so downstream tooling (the `capyc` CLI,
/// the `capy-diagnostics` bridge) can route on the code while still
/// presenting a readable line. Format is stable within v0; snapshot
/// tests should match the code prefix, not the prose tail.
impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

impl std::error::Error for EmitError {}
