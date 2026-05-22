//! Deterministic rustc-style renderer.
//!
//! The output is a stable text block of the form
//!
//! ```text
//! <severity>[<code>]: <message>
//!   --> <file>:<line>:<col>
//!   |
//! <line_num> | <line_text>
//!   |         ^^^^^ <optional label message>
//!   = note: <optional note>
//! ```
//!
//! All offsets are computed against the **byte** width of source bytes so
//! the output is platform-independent. Multi-byte UTF-8 sequences and tab
//! characters are passed through verbatim; downstream rendering may wish
//! to substitute them, but that is outside the S3 contract.

#![forbid(unsafe_code)]

use std::fmt::Write as _;

use capy_lexer::Span;

use crate::diagnostic::{Diagnostic, Label};
use crate::source_map::SourceMap;

/// Renders `diagnostic` against `source`, attributing line/column to the
/// given `file` path (use `"<input>"` for stdin-style invocations).
#[must_use]
pub fn render(diagnostic: &Diagnostic, source: &str, file: &str) -> String {
    let map = SourceMap::new(source);
    let mut out = String::new();
    write_header(&mut out, diagnostic);
    write_location(&mut out, &map, diagnostic.primary.span, file);
    let gutter = gutter_width(&map, diagnostic);
    write_label(&mut out, &map, gutter, &diagnostic.primary);
    for secondary in &diagnostic.secondary {
        write_label(&mut out, &map, gutter, secondary);
    }
    for note in &diagnostic.notes {
        writeln!(out, "{} = note: {}", " ".repeat(gutter), note).expect("infallible");
    }
    out
}

fn write_header(out: &mut String, diagnostic: &Diagnostic) {
    writeln!(
        out,
        "{}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.code.as_str(),
        diagnostic.message
    )
    .expect("infallible");
}

fn write_location(out: &mut String, map: &SourceMap<'_>, span: Span, file: &str) {
    let (line, col) = map.position(span.start);
    writeln!(out, "  --> {}:{}:{}", file, line, col).expect("infallible");
}

/// Width of the line-number gutter, chosen so every label aligns under the
/// header.
fn gutter_width(map: &SourceMap<'_>, diagnostic: &Diagnostic) -> usize {
    let mut max_line = map.position(diagnostic.primary.span.start).0;
    for label in &diagnostic.secondary {
        let line = map.position(label.span.start).0;
        if line > max_line {
            max_line = line;
        }
    }
    max_line.to_string().len()
}

fn write_label(out: &mut String, map: &SourceMap<'_>, gutter: usize, label: &Label) {
    let (line, col) = map.position(label.span.start);
    let line_text = map.line_text(line);
    let line_num_str = pad_left(&line.to_string(), gutter);
    writeln!(out, "{} |", " ".repeat(gutter)).expect("infallible");
    writeln!(out, "{} | {}", line_num_str, line_text).expect("infallible");

    let caret_len = caret_length(label.span, line_text, col);
    let caret_offset = col - 1;
    let mut caret = String::with_capacity(caret_offset + caret_len);
    for _ in 0..caret_offset {
        caret.push(' ');
    }
    if caret_len == 0 {
        caret.push('^');
    } else {
        for _ in 0..caret_len {
            caret.push('^');
        }
    }
    match &label.message {
        Some(msg) => {
            writeln!(out, "{} | {} {}", " ".repeat(gutter), caret, msg).expect("infallible")
        }
        None => writeln!(out, "{} | {}", " ".repeat(gutter), caret).expect("infallible"),
    }
}

/// Number of caret characters to emit for `span` on the line whose 1-indexed
/// start column for `span.start` is `col`. Multi-line spans are truncated
/// to the first line. Zero-width spans (e.g. EOF anchors) collapse to a
/// single caret (returns `0` — the caller handles that case).
fn caret_length(span: Span, line_text: &str, col: usize) -> usize {
    let line_start_in_source = span.start.saturating_sub(col - 1);
    let line_end_in_source = line_start_in_source + line_text.len();
    let intersect_start = span.start.max(line_start_in_source);
    let intersect_end = span.end.min(line_end_in_source);
    if intersect_end <= intersect_start {
        return 0;
    }
    intersect_end - intersect_start
}

fn pad_left(s: &str, width: usize) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let mut out = String::with_capacity(width);
        for _ in 0..(width - s.len()) {
            out.push(' ');
        }
        out.push_str(s);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::diagnostic::{Code, Diagnostic, L_UNTERMINATED_STRING};
    use capy_lexer::Span;

    #[test]
    fn renders_unterminated_string() {
        let source = "\"oops\n";
        let diag = Diagnostic::error(
            L_UNTERMINATED_STRING,
            "unterminated string literal",
            Span::new(0, 5),
        )
        .with_label_message("string opened here");
        let out = render(&diag, source, "<input>");
        let expected = concat!(
            "error[L0001]: unterminated string literal\n",
            "  --> <input>:1:1\n",
            "  |\n",
            "1 | \"oops\n",
            "  | ^^^^^ string opened here\n",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn renders_with_note() {
        let source = "let x = ;\n";
        let diag = Diagnostic::error(
            Code("P0001"),
            "unexpected token: found Semicolon, expected expression",
            Span::new(8, 9),
        )
        .with_label_message("expected expression")
        .with_note("the right-hand side of `=` must be a non-empty expression");
        let out = render(&diag, source, "<input>");
        let expected = concat!(
            "error[P0001]: unexpected token: found Semicolon, expected expression\n",
            "  --> <input>:1:9\n",
            "  |\n",
            "1 | let x = ;\n",
            "  |         ^ expected expression\n",
            "  = note: the right-hand side of `=` must be a non-empty expression\n",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn renders_eof_span() {
        // Empty source: EOF at byte 0.
        let source = "";
        let diag = Diagnostic::error(
            Code("P0002"),
            "unexpected end of input, expected expression",
            Span::new(0, 0),
        )
        .with_label_message("expected expression");
        let out = render(&diag, source, "<input>");
        let expected = concat!(
            "error[P0002]: unexpected end of input, expected expression\n",
            "  --> <input>:1:1\n",
            "  |\n",
            "1 | \n",
            "  | ^ expected expression\n",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn renders_span_on_second_line() {
        let source = "let x = 1;\nlet y = ;\n";
        let diag = Diagnostic::error(Code("P0001"), "unexpected token", Span::new(19, 20))
            .with_label_message("expected expression");
        let out = render(&diag, source, "<input>");
        let expected = concat!(
            "error[P0001]: unexpected token\n",
            "  --> <input>:2:9\n",
            "  |\n",
            "2 | let y = ;\n",
            "  |         ^ expected expression\n",
        );
        assert_eq!(out, expected);
    }
}
