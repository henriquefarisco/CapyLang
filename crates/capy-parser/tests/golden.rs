//! Golden tests for the CapyLang parser (slice S2).
//!
//! Each `.cl` fixture under `tests/fixtures/parser/` must have a paired
//! `.ast` file containing the canonical [`capy_ast::dump_expr`] output of
//! parsing the fixture as a single expression. Diagnostics, when any, are
//! appended at the tail of the dump using a stable text format so a single
//! file fully describes the parser's contract for that input.
//!
//! ## Updating goldens
//!
//! When a fixture is added or the parser output changes intentionally, run:
//!
//! ```text
//! CAPY_GOLDEN_UPDATE=1 cargo test -p capy-parser --test golden
//! ```
//!
//! This rewrites the `.ast` files in place; the test will then pass.

use std::fs;
use std::path::{Path, PathBuf};

use capy_ast::dump_expr;
use capy_parser::{parse_expr, ParseDiagnostic, ParseErrorKind};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parser")
}

fn collect_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("cl"))
        .collect();
    out.sort();
    out
}

fn dump_diagnostics(diags: &[ParseDiagnostic]) -> String {
    if diags.is_empty() {
        return String::new();
    }
    let mut out = String::from("--- diagnostics ---\n");
    for d in diags {
        out.push_str(&format!(
            "[{}..{}] {}\n",
            d.span.start,
            d.span.end,
            render_kind(&d.kind)
        ));
    }
    out
}

fn render_kind(kind: &ParseErrorKind) -> String {
    match kind {
        ParseErrorKind::UnexpectedToken { found, expected } => {
            format!("UnexpectedToken found={found:?} expected={expected}")
        }
        ParseErrorKind::UnexpectedEof { expected } => {
            format!("UnexpectedEof expected={expected}")
        }
        ParseErrorKind::Lex(kind) => format!("Lex {kind:?}"),
    }
}

#[test]
fn fixtures_exist() {
    let entries = collect_fixtures();
    assert!(
        !entries.is_empty(),
        "no .cl fixtures found in {}",
        fixtures_dir().display()
    );
}

#[test]
fn golden() {
    let update = std::env::var_os("CAPY_GOLDEN_UPDATE").is_some();
    let mut failures: Vec<String> = Vec::new();

    for input_path in collect_fixtures() {
        let source = fs::read_to_string(&input_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", input_path.display()));
        let result = parse_expr(&source);
        let mut actual = dump_expr(&result.expr);
        actual.push_str(&dump_diagnostics(&result.diagnostics));
        let expected_path = input_path.with_extension("ast");

        if update {
            fs::write(&expected_path, &actual)
                .unwrap_or_else(|e| panic!("write {}: {e}", expected_path.display()));
            continue;
        }

        let expected = match fs::read_to_string(&expected_path) {
            Ok(s) => s,
            Err(_) => {
                failures.push(format!(
                    "missing golden for {} (run with CAPY_GOLDEN_UPDATE=1 to create)",
                    input_path.display()
                ));
                continue;
            }
        };

        if actual != expected {
            failures.push(render_diff(&input_path, &expected, &actual));
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} golden mismatch(es):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

fn render_diff(path: &Path, expected: &str, actual: &str) -> String {
    let mut out = String::new();
    out.push_str("--- ");
    out.push_str(path.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
    out.push_str(" ---\n");
    out.push_str("expected:\n");
    out.push_str(expected);
    out.push_str("\nactual:\n");
    out.push_str(actual);
    out
}
