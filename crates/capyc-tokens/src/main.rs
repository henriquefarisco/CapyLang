//! `capyc-tokens` - debugging CLI that prints the canonical lexer dump.
//!
//! This binary is a thin wrapper over [`capy_lexer::tokenize`] and
//! [`capy_lexer::dump_tokens`]. It is intentionally tiny: no clap, no JSON
//! output, no colours. The default dump format is the one consumed by the
//! golden tests under `crates/capy-lexer/tests/fixtures/lexer` and is part
//! of the lexer's published contract.
//!
//! ## Usage
//!
//! ```text
//! capyc-tokens [OPTIONS] [PATH]
//! capyc-tokens --help
//! capyc-tokens --version
//! ```
//!
//! Two output modes are supported:
//!
//! - default: canonical token dump (one token per line);
//! - `--counts`: histogram `<count> <Kind>`, sorted by count descending
//!   then by kind name ascending, ideal for shell pipelines.
//!
//! `--no-trivia` filters whitespace, newlines, line and block comments
//! out of the output (counts and dump alike). The synthetic `Eof` token is
//! always retained.
//!
//! When `PATH` is omitted the source is read from standard input. The exit
//! code is `0` when the lexer produced no diagnostics, `1` when at least one
//! diagnostic was attached to the stream (the output is still printed in
//! full), or `2` for usage / I/O errors. The split between `1` and `2` lets
//! shell pipelines distinguish "lex failed" from "no input".

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use capy_lexer::{dump_tokens, tokenize, LexResult, Token, TokenKind};

const USAGE: &str = "\
usage: capyc-tokens [OPTIONS] [PATH]

Print the canonical CapyLang token dump for PATH (or standard input when
PATH is omitted).

OPTIONS:
  -h, --help       Show this help and exit.
  -V, --version    Show the package version and exit.
      --counts     Print a histogram '<count> <Kind>' sorted by count
                   descending, ties broken by kind name ascending.
      --no-trivia  Filter Whitespace, Newline, LineComment and BlockComment
                   from the output. Combines with --counts. The synthetic
                   Eof token is always retained.

EXIT CODES:
  0    No diagnostics emitted.
  1    At least one diagnostic attached to the token stream.
  2    Usage or I/O error (also printed to stderr).
";

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    match parse_args(&args) {
        Ok(Action::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Action::Version) => {
            println!("capyc-tokens {VERSION}");
            ExitCode::SUCCESS
        }
        Ok(Action::Run(opts)) => match read_source(&opts.source) {
            Ok(source) => run(&source, opts.mode, opts.no_trivia),
            Err(err) => {
                eprintln!("capyc-tokens: {err}");
                ExitCode::from(2)
            }
        },
        Err(err) => {
            eprintln!("capyc-tokens: {err}\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// One of the high-level commands `capyc-tokens` can perform.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Version,
    Run(RunOpts),
}

/// Aggregate of all options consumed by the `Run` action.
#[derive(Debug, PartialEq, Eq)]
struct RunOpts {
    source: SourceSpec,
    mode: Mode,
    no_trivia: bool,
}

/// Output mode selected on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Canonical one-token-per-line dump (default).
    Dump,
    /// `<count> <Kind>` histogram sorted by frequency then by kind name.
    Counts,
}

/// Where to read the input from.
#[derive(Debug, PartialEq, Eq)]
enum SourceSpec {
    Stdin,
    File(String),
}

/// Parse the (positional + flag) argv into an [`Action`].
///
/// Kept side-effect free so it can be unit-tested without touching the
/// filesystem. Flags accumulate; positional arguments are validated to be
/// unique.
fn parse_args(args: &[String]) -> Result<Action, String> {
    let mut path: Option<String> = None;
    let mut mode = Mode::Dump;
    let mut no_trivia = false;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--counts" => mode = Mode::Counts,
            "--no-trivia" => no_trivia = true,
            a if a.starts_with('-') => {
                return Err(format!("unknown option: {a}"));
            }
            a => {
                if path.is_some() {
                    return Err("only one PATH argument is accepted".into());
                }
                path = Some(a.to_owned());
            }
        }
    }
    let source = match path {
        Some(p) => SourceSpec::File(p),
        None => SourceSpec::Stdin,
    };
    Ok(Action::Run(RunOpts {
        source,
        mode,
        no_trivia,
    }))
}

/// Materialises the [`SourceSpec`] into an owned `String`.
fn read_source(spec: &SourceSpec) -> Result<String, String> {
    match spec {
        SourceSpec::Stdin => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("reading stdin: {e}"))?;
            Ok(buf)
        }
        SourceSpec::File(path) => {
            fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))
        }
    }
}

/// Runs the lexer and writes the requested view to stdout. Returns the
/// process exit code mirroring the diagnostics count.
fn run(source: &str, mode: Mode, no_trivia: bool) -> ExitCode {
    let mut result = tokenize(source);
    if no_trivia {
        result = filter_trivia(result);
    }
    let output = match mode {
        Mode::Dump => dump_tokens(source, &result),
        Mode::Counts => format_counts(&result.tokens),
    };
    if let Err(err) = io::stdout().write_all(output.as_bytes()) {
        eprintln!("capyc-tokens: writing stdout: {err}");
        return ExitCode::from(2);
    }
    exit_code_for(&result)
}

/// Returns a new [`LexResult`] keeping every non-trivia token plus the
/// synthetic `Eof`. Diagnostics are preserved as-is so the exit code stays
/// consistent between `--no-trivia` and the default mode.
fn filter_trivia(result: LexResult) -> LexResult {
    let LexResult {
        tokens,
        diagnostics,
    } = result;
    let tokens = tokens
        .into_iter()
        .filter(|tok| !tok.kind.is_trivia())
        .collect();
    LexResult {
        tokens,
        diagnostics,
    }
}

/// Renders a canonical histogram for the given token list.
///
/// Format: one line per kind, `<count><space><Kind>\n`. Sorted by count
/// descending, ties broken by the [`Debug`] name of the kind ascending so
/// the output is byte-stable between runs (no `HashMap` iteration order
/// leaks).
fn format_counts(tokens: &[Token]) -> String {
    let mut buckets: HashMap<TokenKind, usize> = HashMap::new();
    for tok in tokens {
        *buckets.entry(tok.kind).or_insert(0) += 1;
    }
    let mut rows: Vec<(String, usize)> = buckets
        .into_iter()
        .map(|(kind, count)| (format!("{kind:?}"), count))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out = String::with_capacity(rows.len() * 16);
    for (kind, count) in rows {
        writeln!(out, "{count} {kind}").expect("writing into String is infallible");
    }
    out
}

/// Pure helper exposed for testing: maps a [`LexResult`] to the documented
/// exit code (0 when clean, 1 when any diagnostic is present).
fn exit_code_for(result: &LexResult) -> ExitCode {
    if result.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_trivia, format_counts, parse_args, Action, Mode, RunOpts, SourceSpec};
    use capy_lexer::{tokenize, TokenKind};

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    fn run_opts(source: SourceSpec, mode: Mode, no_trivia: bool) -> Action {
        Action::Run(RunOpts {
            source,
            mode,
            no_trivia,
        })
    }

    #[test]
    fn no_args_reads_stdin() {
        let action = parse_args(&argv(&[])).expect("parse");
        assert_eq!(action, run_opts(SourceSpec::Stdin, Mode::Dump, false));
    }

    #[test]
    fn single_path_argument() {
        let action = parse_args(&argv(&["foo.cl"])).expect("parse");
        assert_eq!(
            action,
            run_opts(SourceSpec::File("foo.cl".into()), Mode::Dump, false)
        );
    }

    #[test]
    fn help_flag_short_and_long() {
        assert_eq!(parse_args(&argv(&["-h"])).expect("parse"), Action::Help);
        assert_eq!(parse_args(&argv(&["--help"])).expect("parse"), Action::Help);
    }

    #[test]
    fn version_flag_short_and_long() {
        assert_eq!(parse_args(&argv(&["-V"])).expect("parse"), Action::Version);
        assert_eq!(
            parse_args(&argv(&["--version"])).expect("parse"),
            Action::Version
        );
    }

    #[test]
    fn unknown_flag_is_an_error() {
        let err = parse_args(&argv(&["--frobnicate"])).unwrap_err();
        assert!(err.contains("unknown option"), "got {err}");
    }

    #[test]
    fn second_path_is_rejected() {
        let err = parse_args(&argv(&["a.cl", "b.cl"])).unwrap_err();
        assert!(err.contains("only one PATH"), "got {err}");
    }

    #[test]
    fn counts_flag_switches_mode() {
        let action = parse_args(&argv(&["--counts", "foo.cl"])).expect("parse");
        assert_eq!(
            action,
            run_opts(SourceSpec::File("foo.cl".into()), Mode::Counts, false)
        );
    }

    #[test]
    fn no_trivia_flag_sets_bit() {
        let action = parse_args(&argv(&["--no-trivia"])).expect("parse");
        assert_eq!(action, run_opts(SourceSpec::Stdin, Mode::Dump, true));
    }

    #[test]
    fn counts_and_no_trivia_compose() {
        let action = parse_args(&argv(&["--counts", "--no-trivia", "foo.cl"])).expect("parse");
        assert_eq!(
            action,
            run_opts(SourceSpec::File("foo.cl".into()), Mode::Counts, true)
        );
    }

    #[test]
    fn flag_order_is_irrelevant() {
        let a = parse_args(&argv(&["--no-trivia", "--counts", "foo.cl"])).expect("parse");
        let b = parse_args(&argv(&["foo.cl", "--counts", "--no-trivia"])).expect("parse");
        assert_eq!(a, b);
    }

    #[test]
    fn filter_trivia_drops_only_trivia_kinds() {
        // `let x = 42` produces idents, a keyword, an operator, an int and
        // whitespace. The synthetic Eof must survive the filter.
        let result = tokenize("let x = 42");
        let trivia_count = result.tokens.iter().filter(|t| t.kind.is_trivia()).count();
        assert!(trivia_count > 0, "fixture should contain trivia");

        let filtered = filter_trivia(result);
        assert!(filtered.tokens.iter().all(|t| !t.kind.is_trivia()));
        assert_eq!(
            filtered.tokens.last().map(|t| t.kind),
            Some(TokenKind::Eof),
            "Eof must be retained"
        );
    }

    #[test]
    fn filter_trivia_preserves_diagnostics() {
        // An unterminated string emits a diagnostic; filter must keep it so
        // the CLI exit code does not silently flip from 1 to 0.
        let result = tokenize("\"oops");
        assert_eq!(result.diagnostics.len(), 1);

        let filtered = filter_trivia(result);
        assert_eq!(filtered.diagnostics.len(), 1);
    }

    #[test]
    fn format_counts_is_deterministic_and_sorted() {
        let result = tokenize("let x = 1 + 1");
        let output = format_counts(&result.tokens);

        // First line wins the count race; ties resolve by Kind name asc.
        let lines: Vec<&str> = output.lines().collect();
        assert!(!lines.is_empty());

        // Counts must be monotonically non-increasing.
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| {
                l.split_whitespace()
                    .next()
                    .expect("count column")
                    .parse::<usize>()
                    .expect("numeric count")
            })
            .collect();
        for window in counts.windows(2) {
            assert!(window[0] >= window[1], "counts not sorted desc: {counts:?}");
        }

        // Tie-breaking: equal-count rows are sorted by Kind name asc.
        for window in lines.windows(2) {
            let (c0, n0) = split_count_line(window[0]);
            let (c1, n1) = split_count_line(window[1]);
            if c0 == c1 {
                assert!(n0 <= n1, "tie-broken order wrong: {n0:?} vs {n1:?}");
            }
        }
    }

    fn split_count_line(line: &str) -> (usize, String) {
        let mut parts = line.splitn(2, ' ');
        let count = parts.next().unwrap().parse().unwrap();
        let name = parts.next().unwrap().to_owned();
        (count, name)
    }

    #[test]
    fn format_counts_total_matches_token_stream() {
        let result = tokenize("fn f() { 1 + 2 }");
        let output = format_counts(&result.tokens);
        let total: usize = output
            .lines()
            .map(|l| {
                l.split_whitespace()
                    .next()
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            })
            .sum();
        assert_eq!(total, result.tokens.len());
    }
}
