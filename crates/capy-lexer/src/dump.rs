//! Canonical textual dump of a [`LexResult`].
//!
//! Used by golden tests under `crates/capy-lexer/tests/golden.rs` and by
//! tooling such as `capyc tokens` (added in S12). The format is intentionally
//! tiny so it can be diffed by humans and by `git`:
//!
//! ```text
//! [<start>..<end>] <Kind>
//! [<start>..<end>] <Kind> "<text>"
//! [<start>..<end>] Error <ErrorKind> "<text>"
//! ```
//!
//! Stability: this format is part of the lexer's contract. Changes between
//! minor versions must remain additive (new optional trailers).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::diagnostic::{Diagnostic, LexErrorKind};
use crate::lexer::{slice_safe, LexResult};
use crate::token::TokenKind;

/// Renders `result` into the canonical text format documented above.
#[must_use]
pub fn dump_tokens(source: &str, result: &LexResult) -> String {
    let diag_index: HashMap<usize, LexErrorKind> = result
        .diagnostics
        .iter()
        .map(|d| (d.span.start, d.kind))
        .collect();

    let mut out = String::with_capacity(result.tokens.len() * 24);
    for token in &result.tokens {
        write!(
            out,
            "[{}..{}] {:?}",
            token.span.start, token.span.end, token.kind
        )
        .expect("writing into String is infallible");

        match token.kind {
            TokenKind::Error => {
                if let Some(kind) = diag_index.get(&token.span.start) {
                    write!(out, " {kind:?}").expect("infallible");
                }
                let text = slice_safe(source, token.span);
                write!(out, " {text:?}").expect("infallible");
            }
            kind if kind.carries_text() => {
                let text = slice_safe(source, token.span);
                write!(out, " {text:?}").expect("infallible");
            }
            _ => {}
        }
        out.push('\n');
    }

    // Append a tail listing diagnostics that were never attached to a token
    // (should be unreachable in practice; keeps the dump self-describing).
    let unmatched: Vec<&Diagnostic> = result
        .diagnostics
        .iter()
        .filter(|d| !result.tokens.iter().any(|t| t.span.start == d.span.start))
        .collect();
    if !unmatched.is_empty() {
        out.push_str("--- orphan diagnostics ---\n");
        for d in unmatched {
            writeln!(out, "[{}..{}] {:?}", d.span.start, d.span.end, d.kind).expect("infallible");
        }
    }

    out
}
