//! Golden tests for the CapyLang lexer.
//!
//! Each `.cl` fixture under `tests/fixtures/lexer/` must have a paired
//! `.tokens` file containing the canonical [`capy_lexer::dump_tokens`]
//! output. The test compares actual against expected and reports every
//! mismatch in a single panic message.
//!
//! ## Updating goldens
//!
//! When a fixture is added or the lexer output changes intentionally, run:
//!
//! ```text
//! CAPY_GOLDEN_UPDATE=1 cargo test -p capy-lexer --test golden
//! ```
//!
//! This rewrites the `.tokens` files in place; the test will then pass.

use std::fs;
use std::path::{Path, PathBuf};

use capy_lexer::{dump_tokens, tokenize};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lexer")
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
        let result = tokenize(&source);
        let actual = dump_tokens(&source, &result);
        let expected_path = input_path.with_extension("tokens");

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
