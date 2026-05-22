//! Bridges from the lexer (S1) and parser (S2) per-stage diagnostic types
//! to the unified [`Diagnostic`] of slice S3.
//!
//! The mappings are deterministic and pure; no allocation is performed
//! beyond the necessary `String` fields. Stable [`Code`](crate::Code)
//! values are taken from [`crate::diagnostic`].

#![forbid(unsafe_code)]

use capy_bytecode::DebugInfo;
use capy_emitter::EmitError;
use capy_lexer::{Diagnostic as LexDiagnostic, LexErrorKind, Span};
use capy_parser::{ParseDiagnostic, ParseErrorKind};
use capy_vm::VmError;

use crate::diagnostic::{
    Code, Diagnostic, L_UNKNOWN_CHAR, L_UNTERMINATED_BLOCK_COMMENT, L_UNTERMINATED_STRING,
    P_UNEXPECTED_EOF, P_UNEXPECTED_TOKEN,
};

/// Converts a lexer [`Diagnostic`](capy_lexer::Diagnostic) into the unified
/// [`Diagnostic`] shape.
#[must_use]
pub fn from_lex(diagnostic: &LexDiagnostic) -> Diagnostic {
    let (code, message, label_message) = match diagnostic.kind {
        LexErrorKind::UnterminatedString => (
            L_UNTERMINATED_STRING,
            "unterminated string literal",
            "string opened here",
        ),
        LexErrorKind::UnterminatedBlockComment => (
            L_UNTERMINATED_BLOCK_COMMENT,
            "unterminated block comment",
            "block comment opened here",
        ),
        LexErrorKind::UnknownChar => (
            L_UNKNOWN_CHAR,
            "unknown character",
            "no token starts with this byte sequence",
        ),
    };
    Diagnostic::error(code, message, diagnostic.span).with_label_message(label_message)
}

/// Converts a parser [`ParseDiagnostic`] into the unified [`Diagnostic`]
/// shape. `ParseErrorKind::Lex` is recursively bridged through [`from_lex`].
#[must_use]
pub fn from_parse(diagnostic: &ParseDiagnostic) -> Diagnostic {
    match &diagnostic.kind {
        ParseErrorKind::UnexpectedToken { found, expected } => Diagnostic::error(
            P_UNEXPECTED_TOKEN,
            format!("unexpected token: found {found:?}, expected {expected}"),
            diagnostic.span,
        )
        .with_label_message(format!("expected {expected}")),
        ParseErrorKind::UnexpectedEof { expected } => Diagnostic::error(
            P_UNEXPECTED_EOF,
            format!("unexpected end of input, expected {expected}"),
            diagnostic.span,
        )
        .with_label_message(format!("expected {expected}")),
        ParseErrorKind::Lex(kind) => {
            let lex = LexDiagnostic::new(*kind, diagnostic.span);
            from_lex(&lex)
        }
    }
}

/// Converts an [`EmitError`] into the unified [`Diagnostic`] shape.
///
/// The error's stable `E<NNNN>` code is preserved verbatim and the
/// human-readable message comes from [`EmitError::message`] so it does
/// not double-print the code (the renderer already emits
/// `error[E<NNNN>]: <message>`). The primary span is the
/// `EmitError::span` from the offending AST node.
#[must_use]
pub fn from_emit(diagnostic: &EmitError) -> Diagnostic {
    Diagnostic::error(Code(diagnostic.code()), diagnostic.message(), diagnostic.span)
}

/// Converts a [`VmError`] into the unified [`Diagnostic`] shape.
///
/// The VM does not carry a source span (its errors are addressed by
/// `pc` inside the bytecode stream). Without a `DebugInfo` lookup
/// table the bridge cannot resolve `pc` back to a source byte range,
/// so the produced [`Diagnostic`] uses an empty primary span
/// (`Span::new(0, 0)`). Downstream tooling that has access to the
/// module's `DebugInfo` may resolve the span itself and replace
/// `primary.span` before rendering.
///
/// The Display impl on [`VmError`] is the source of truth for the
/// human-readable text; the bridge strips its leading `[V<NNNN>] `
/// prefix to avoid double-printing the code (which the renderer
/// already inserts as `error[V<NNNN>]: <message>`). The `pc` and
/// other addresses remain in the message tail so the diagnostic
/// stays self-contained even without a `DebugInfo` resolver.
#[must_use]
pub fn from_vm(error: &VmError) -> Diagnostic {
    from_vm_with_span(error, Span::new(0, 0))
}

/// Like [`from_vm`] but resolves the VM's `pc` to a source span using
/// the module's [`DebugInfo`].
///
/// The lookup uses nearest-not-greater semantics: the entry with the
/// largest `bytecode_offset` ≤ `error.pc()` wins. This mirrors the
/// emitter's convention of recording a `DebugEntry` per instruction;
/// any pc between two adjacent entries belongs to the earlier one's
/// instruction.
///
/// Falls back to [`Span::new(0, 0)`] (the same shape as [`from_vm`])
/// when the error variant has no `pc` (`BudgetExhausted`,
/// `UnknownFunction`, `MalformedModule`), when the debug info has no
/// suitable entry, or when the module did not ship a `Debug` section.
///
/// **v0 limitation**: the wire `DebugEntry` records `bytecode_offset`
/// relative to a function's code, but does not record which function.
/// The emitter today emits debug entries for function index `0` only;
/// `from_vm_with_debug` is therefore accurate for traps inside the
/// entry-point function and approximate (or empty) for traps inside
/// other functions. A v1 debug section will add a function-index
/// field; the bridge signature is forward-compatible.
#[must_use]
pub fn from_vm_with_debug(error: &VmError, debug: &DebugInfo) -> Diagnostic {
    let span = error
        .pc()
        .and_then(|pc| resolve_pc(pc, debug))
        .unwrap_or_else(|| Span::new(0, 0));
    from_vm_with_span(error, span)
}

/// Internal helper used by [`from_vm`] and [`from_vm_with_debug`].
fn from_vm_with_span(error: &VmError, span: Span) -> Diagnostic {
    let code = error.code();
    let full = error.to_string();
    let prefix = format!("[{code}] ");
    let message = full
        .strip_prefix(&prefix)
        .map(str::to_string)
        .unwrap_or(full);
    Diagnostic::error(Code(code), message, span)
}

/// Looks up the source span associated with `pc` in `debug`.
///
/// Returns the [`Span`] of the entry whose `bytecode_offset` is the
/// largest value ≤ `pc`, or `None` when no entry qualifies (e.g. the
/// debug section is empty or the smallest `bytecode_offset` is already
/// greater than `pc`). The scan is linear in `entries.len()`; the
/// emitter records entries in increasing offset order so a binary
/// search is possible if profiles ever justify it.
fn resolve_pc(pc: u32, debug: &DebugInfo) -> Option<Span> {
    let mut best: Option<&capy_bytecode::DebugEntry> = None;
    for entry in &debug.entries {
        if entry.bytecode_offset > pc {
            break;
        }
        best = Some(entry);
    }
    best.map(|e| Span::new(e.source_start as usize, e.source_end as usize))
}

#[cfg(test)]
mod tests {
    use super::{from_lex, from_parse};
    use crate::diagnostic::{
        Severity, L_UNKNOWN_CHAR, L_UNTERMINATED_STRING, P_UNEXPECTED_EOF, P_UNEXPECTED_TOKEN,
    };
    use capy_lexer::{Diagnostic as LexDiagnostic, LexErrorKind, Span, TokenKind};
    use capy_parser::{ParseDiagnostic, ParseErrorKind};

    #[test]
    fn lex_unterminated_string_maps_to_l0001() {
        let lex = LexDiagnostic::new(LexErrorKind::UnterminatedString, Span::new(0, 5));
        let diag = from_lex(&lex);
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, L_UNTERMINATED_STRING);
        assert_eq!(diag.message, "unterminated string literal");
    }

    #[test]
    fn lex_unknown_char_maps_to_l0003() {
        let lex = LexDiagnostic::new(LexErrorKind::UnknownChar, Span::new(0, 1));
        let diag = from_lex(&lex);
        assert_eq!(diag.code, L_UNKNOWN_CHAR);
    }

    #[test]
    fn parse_unexpected_token_maps_to_p0001() {
        let parse = ParseDiagnostic::new(
            ParseErrorKind::UnexpectedToken {
                found: TokenKind::Semicolon,
                expected: "expression",
            },
            Span::new(8, 9),
        );
        let diag = from_parse(&parse);
        assert_eq!(diag.code, P_UNEXPECTED_TOKEN);
        assert!(diag.message.contains("Semicolon"));
        assert!(diag.message.contains("expression"));
        assert_eq!(
            diag.primary.message.as_deref(),
            Some("expected expression")
        );
    }

    #[test]
    fn parse_unexpected_eof_maps_to_p0002() {
        let parse = ParseDiagnostic::new(
            ParseErrorKind::UnexpectedEof { expected: "`;`" },
            Span::new(10, 10),
        );
        let diag = from_parse(&parse);
        assert_eq!(diag.code, P_UNEXPECTED_EOF);
    }

    #[test]
    fn parse_lex_kind_is_routed_through_from_lex() {
        let parse = ParseDiagnostic::new(
            ParseErrorKind::Lex(LexErrorKind::UnterminatedString),
            Span::new(0, 5),
        );
        let diag = from_parse(&parse);
        assert_eq!(diag.code, L_UNTERMINATED_STRING);
    }

    // --- from_emit ---------------------------------------------------

    use super::{from_emit, from_vm};
    use crate::diagnostic::Code;
    use capy_emitter::{EmitError, EmitErrorKind};
    use capy_vm::VmError;

    #[test]
    fn emit_unknown_function_maps_to_e0016_with_span() {
        let err = EmitError::new(
            EmitErrorKind::UnknownFunction {
                name: "foo".to_string(),
            },
            Span::new(10, 13),
        );
        let diag = from_emit(&err);
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Code(err.code()));
        assert_eq!(diag.code, Code("E0016"));
        assert_eq!(diag.primary.span, Span::new(10, 13));
        assert!(diag.message.contains("foo"));
        // The structured message must NOT carry the `[E<NNNN>] `
        // prefix (the renderer prints the code in the header).
        assert!(!diag.message.starts_with("[E0016]"));
    }

    #[test]
    fn emit_duplicate_import_threads_name() {
        let err = EmitError::new(
            EmitErrorKind::DuplicateImport {
                name: "now".to_string(),
            },
            Span::new(0, 5),
        );
        let diag = from_emit(&err);
        assert_eq!(diag.code, Code("E0020"));
        assert!(diag.message.contains("now"));
    }

    #[test]
    fn emit_message_strips_code_prefix_from_display() {
        // Defence-in-depth: even if a future variant relied on Display
        // for its prose, the bridge would still hand the renderer
        // prefix-free text. Check via the formatted Display form.
        let err = EmitError::new(
            EmitErrorKind::UnsupportedFeature {
                what: "match expressions",
            },
            Span::new(2, 7),
        );
        let display = format!("{err}");
        assert!(display.starts_with("[E0010]"));
        let diag = from_emit(&err);
        assert!(!diag.message.starts_with("[E"));
    }

    // --- from_vm ----------------------------------------------------

    #[test]
    fn vm_division_by_zero_maps_to_v0006() {
        let err = VmError::DivisionByZero { pc: 0x10 };
        let diag = from_vm(&err);
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Code("V0006"));
        // pc tail is preserved in the message body so the diagnostic
        // remains self-contained without a DebugInfo resolver.
        assert!(diag.message.contains("pc=0x0010"));
        // Prefix `[V0006] ` was stripped.
        assert!(!diag.message.starts_with('['));
    }

    #[test]
    fn vm_default_span_is_empty_at_origin() {
        let err = VmError::StackUnderflow { pc: 0 };
        let diag = from_vm(&err);
        assert_eq!(diag.primary.span, Span::new(0, 0));
    }

    #[test]
    fn vm_malformed_module_threads_inner_bytecode_code_into_message() {
        let err = VmError::MalformedModule {
            reason: "header magic mismatch",
            code: "B0001",
        };
        let diag = from_vm(&err);
        assert_eq!(diag.code, Code("V0009"));
        assert!(diag.message.contains("B0001"));
        assert!(diag.message.contains("header magic mismatch"));
    }

    #[test]
    fn vm_host_call_failed_preserves_reason_verbatim() {
        let err = VmError::HostCallFailed {
            pc: 12,
            module: "log".to_string(),
            symbol: "info".to_string(),
            reason: "log::info expects a Str argument",
        };
        let diag = from_vm(&err);
        assert_eq!(diag.code, Code("V0016"));
        assert!(diag.message.contains("log::info"));
        assert!(diag.message.contains("expects a Str argument"));
    }

    // --- from_vm_with_debug -----------------------------------------

    use super::from_vm_with_debug;
    use capy_bytecode::{DebugEntry, DebugInfo};

    fn debug(entries: &[(u32, u32, u32)]) -> DebugInfo {
        DebugInfo {
            entries: entries
                .iter()
                .map(|&(bc, s, e)| DebugEntry {
                    bytecode_offset: bc,
                    source_start: s,
                    source_end: e,
                })
                .collect(),
        }
    }

    #[test]
    fn vm_with_debug_resolves_pc_to_nearest_not_greater_entry() {
        // pc=12 lands between bytecode_offsets 10 and 20; the lookup
        // picks the entry at 10, whose source span is [42, 47).
        let dbg = debug(&[(0, 0, 5), (10, 42, 47), (20, 100, 110)]);
        let err = VmError::StackUnderflow { pc: 12 };
        let diag = from_vm_with_debug(&err, &dbg);
        assert_eq!(diag.primary.span, Span::new(42, 47));
        assert_eq!(diag.code, Code("V0001"));
    }

    #[test]
    fn vm_with_debug_exact_offset_match_picks_that_entry() {
        let dbg = debug(&[(0, 0, 1), (5, 5, 9), (10, 12, 18)]);
        let err = VmError::DivisionByZero { pc: 5 };
        let diag = from_vm_with_debug(&err, &dbg);
        assert_eq!(diag.primary.span, Span::new(5, 9));
    }

    #[test]
    fn vm_with_debug_pc_before_first_entry_falls_back_to_origin() {
        // bytecode_offset 100 is the only entry; pc=0 has nothing
        // ≤ it, so the bridge returns the (0,0) sentinel.
        let dbg = debug(&[(100, 200, 210)]);
        let err = VmError::StackUnderflow { pc: 0 };
        let diag = from_vm_with_debug(&err, &dbg);
        assert_eq!(diag.primary.span, Span::new(0, 0));
    }

    #[test]
    fn vm_with_debug_pcless_variants_fall_back_to_origin() {
        // BudgetExhausted has no pc, so even a populated DebugInfo
        // cannot supply a span. The bridge returns the same sentinel
        // as `from_vm`.
        let dbg = debug(&[(0, 1, 2)]);
        let err = VmError::BudgetExhausted { budget: 1 };
        let diag = from_vm_with_debug(&err, &dbg);
        assert_eq!(diag.code, Code("V0007"));
        assert_eq!(diag.primary.span, Span::new(0, 0));
    }

    #[test]
    fn vm_with_debug_empty_debug_info_is_safe() {
        let dbg = DebugInfo::new();
        let err = VmError::StackUnderflow { pc: 42 };
        let diag = from_vm_with_debug(&err, &dbg);
        assert_eq!(diag.primary.span, Span::new(0, 0));
    }

    #[test]
    fn vm_error_pc_accessor_round_trip() {
        // Sanity check the new VmError::pc accessor used by the bridge.
        assert_eq!(
            VmError::StackUnderflow { pc: 0x10 }.pc(),
            Some(0x10)
        );
        assert!(VmError::BudgetExhausted { budget: 0 }.pc().is_none());
        assert!(VmError::UnknownFunction {
            name: "x".to_string()
        }
        .pc()
        .is_none());
        assert!(VmError::MalformedModule {
            reason: "r",
            code: "B0001",
        }
        .pc()
        .is_none());
    }
}
