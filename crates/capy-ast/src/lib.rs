//! CapyLang abstract syntax tree (slice S2).
//!
//! Every AST node carries the byte [`Span`] of the source range it covers.
//! Spans are reused from [`capy_lexer::Span`] so downstream consumers see a
//! single span type throughout the lexer / parser / future bytecode emitter
//! pipeline.
//!
//! S2.0 covers the **expression** sublanguage only: literals, identifiers and
//! paths, parenthesised expressions, function calls, indexing, field access,
//! unary and binary operators with C-like precedence. Items, statements,
//! control flow and declarations are added in subsequent S2.x slices.
//!
//! # Example
//!
//! ```
//! use capy_ast::{Expr, dump_expr};
//! use capy_lexer::Span;
//!
//! let expr = Expr::Int {
//!     text: "42".to_string(),
//!     span: Span::new(0, 2),
//! };
//! assert_eq!(dump_expr(&expr), "[0..2] Int \"42\"\n");
//! ```

#![forbid(unsafe_code)]

mod dump;
mod expr;

pub use capy_lexer::Span;
pub use dump::{dump_expr, dump_source};
pub use expr::{
    BinOp, ConstItem, EnumItem, Expr, FnItem, Ident, ImportItem, Item, MatchArm, Param, Pattern,
    Source, Stmt, StructField, StructItem, StructPatternField, Type, TypeAlias, UnOp, Variant,
    VariantBody,
};
