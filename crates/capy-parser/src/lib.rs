//! CapyLang parser (slice S2).
//!
//! Hand-written recursive-descent parser producing a span-preserving
//! [`Expr`](capy_ast::Expr) tree. The parser is **deterministic** and
//! **fail-closed**: every parse outcome is reproducible from the input
//! bytes, malformed input is reported via [`ParseDiagnostic`] and the
//! parser never panics on user input. Lexer diagnostics surfaced during
//! parsing are wrapped as [`ParseErrorKind::Lex`].
//!
//! S2.0 covers expressions only (literals, identifiers, paths, parens,
//! calls, indexing, field access, unary and binary operators with C-like
//! precedence). Statements, items and control flow follow in later S2.x
//! slices.
//!
//! # Example
//!
//! ```
//! use capy_parser::parse_expr;
//!
//! let result = parse_expr("1 + 2 * 3");
//! assert!(result.diagnostics.is_empty());
//! ```

#![forbid(unsafe_code)]

mod diagnostic;
mod parser;

pub use capy_ast::{
    dump_expr, dump_source, BinOp, ConstItem, EnumItem, Expr, FnItem, Ident, ImportItem, Item,
    MatchArm, Param, Pattern, Source, Span, Stmt, StructField, StructItem, StructPatternField,
    Type, TypeAlias, UnOp, Variant, VariantBody,
};
pub use diagnostic::{ParseDiagnostic, ParseErrorKind};
pub use parser::{parse_expr, parse_source, ParseResult, ParseSourceResult};
