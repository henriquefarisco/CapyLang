//! CapyLang structured diagnostics (slice S3).
//!
//! Provides a unified [`Diagnostic`] type with severity, stable
//! per-message [`Code`], primary and secondary [`Label`]s, optional notes,
//! a [`SourceMap`] for byte → `(line, column)` translation, and a
//! deterministic [`render`] function that emits a rustc-like text block.
//!
//! Bridges from the per-stage diagnostic types live in
//! [`bridge::from_lex`], [`bridge::from_parse`], [`bridge::from_emit`]
//! and [`bridge::from_vm`]; downstream tooling (`capyc check` in S12,
//! IDE adapters, the CapyOS Etapa-15 adapter) can rely on the unified
//! shape without inspecting the lexer / parser / emitter / VM
//! internal types.
//!
//! # Example
//!
//! ```
//! use capy_diagnostics::{render, Diagnostic, Severity, Code};
//! use capy_lexer::Span;
//!
//! let diag = Diagnostic::error(
//!     Code("L0003"),
//!     "unknown character",
//!     Span::new(0, 1),
//! );
//! let out = render(&diag, "?", "<input>");
//! assert!(out.starts_with("error[L0003]: unknown character\n"));
//! assert_eq!(diag.severity, Severity::Error);
//! ```

#![forbid(unsafe_code)]

mod bridge;
mod diagnostic;
mod render;
mod source_map;

pub use bridge::{from_emit, from_lex, from_parse, from_vm, from_vm_with_debug};
pub use diagnostic::{
    Code, Diagnostic, Label, Severity, L_UNKNOWN_CHAR, L_UNTERMINATED_BLOCK_COMMENT,
    L_UNTERMINATED_STRING, P_UNEXPECTED_EOF, P_UNEXPECTED_TOKEN,
};
pub use render::render;
pub use source_map::SourceMap;
