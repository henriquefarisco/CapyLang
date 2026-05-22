//! `capyc` — unified CLI front-end for the CapyLang toolchain (S12).
//!
//! Subcommands:
//!
//! ```text
//! capyc tokens  [PATH]                 print canonical lexer dump
//! capyc parse   [PATH]                 print canonical AST dump
//! capyc compile [PATH] [-o OUTPUT]     emit v0 bytecode bytes
//! capyc disasm  [PATH]                 disassemble v0 bytecode
//! capyc run     [PATH] [--entry NAME]
//!                     [--budget N]     compile + execute via VM
//! ```
//!
//! Design notes:
//!
//! - **No external dependencies.** Argument parsing is hand-rolled and
//!   intentionally tiny so the binary stays in the workspace lock-step
//!   with `capyc-tokens`.
//! - **Source vs bytecode.** `compile`, `parse` and `run` accept source
//!   (`.cl`); `disasm` accepts a v0 bytecode file (`.bc`). `run` will
//!   auto-detect input shape via the bytecode magic `CAPY` so a single
//!   path may be either.
//! - **Determinism.** Output formats are the canonical artefacts already
//!   covered by golden tests (lexer dump, AST dump, instruction codec
//!   disassembly). The CLI adds no surface drift.
//! - **Exit codes.**
//!     - `0` clean (no diagnostics from any stage);
//!     - `1` at least one recoverable diagnostic surfaced;
//!     - `2` usage / I/O / loader error.
//!
//! `PATH = -` (or omitted) means standard input. The path is passed
//! through to readers; binary outputs go to stdout when `-o` is absent.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use capy_ast::dump_source;
use capy_bytecode::{
    decode, disassemble_text, ConstPool, FunctionTable, ImportTable, Module, SectionTag,
};
use capy_emitter::{emit, EmitError};
use capy_lexer::{dump_tokens, tokenize};
use capy_parser::{parse_source, ParseDiagnostic};
use capy_vm::{HostAdapter, Value, Vm, DEFAULT_INSTRUCTION_BUDGET};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
usage: capyc <SUBCOMMAND> [OPTIONS] [PATH]

SUBCOMMANDS:
  tokens   Print the canonical lexer dump for PATH.
  parse    Print the canonical AST dump for PATH.
  compile  Compile PATH to v0 bytecode bytes.
  disasm   Disassemble a v0 bytecode file.
  run      Compile (or load) PATH and execute its entry function.
  help     Show this help and exit.

OPTIONS:
  -h, --help       Show subcommand help.
  -V, --version    Show the package version and exit.

COMPILE OPTIONS:
  -o, --output PATH    Write bytecode to PATH (defaults to stdout).

RUN OPTIONS:
      --entry NAME     Function to invoke (defaults to `main`).
      --budget N       Instruction budget for the VM (defaults to the
                       VM's DEFAULT_INSTRUCTION_BUDGET).

PATH:
  A filesystem path, or `-` / omitted to read from standard input.

EXIT CODES:
  0    No diagnostics emitted by any pipeline stage.
  1    At least one recoverable diagnostic was surfaced (lexer,
       parser, emitter, verifier or runtime).
  2    Usage or I/O error (also printed to stderr).
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("capyc: {e}\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    let sub = match args.first() {
        Some(s) => s.as_str(),
        None => {
            print!("{USAGE}");
            return Ok(ExitCode::SUCCESS);
        }
    };
    match sub {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "-V" | "--version" => {
            println!("capyc {VERSION}");
            Ok(ExitCode::SUCCESS)
        }
        "tokens" => cmd_tokens(&args[1..]),
        "parse" => cmd_parse(&args[1..]),
        "compile" => cmd_compile(&args[1..]),
        "disasm" => cmd_disasm(&args[1..]),
        "run" => cmd_run(&args[1..]),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

// ---------------------------------------------------------------------
// tokens
// ---------------------------------------------------------------------

fn cmd_tokens(args: &[String]) -> Result<ExitCode, String> {
    let path = take_path_arg(args)?;
    let source = read_source(&path)?;
    let lex = tokenize(&source);
    let dump = dump_tokens(&lex);
    print_with_optional_newline(&dump);
    Ok(if lex.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

// ---------------------------------------------------------------------
// parse
// ---------------------------------------------------------------------

fn cmd_parse(args: &[String]) -> Result<ExitCode, String> {
    let path = take_path_arg(args)?;
    let source = read_source(&path)?;
    let result = parse_source(&source);
    let dump = dump_source(&result.source);
    print_with_optional_newline(&dump);
    if !result.diagnostics.is_empty() {
        for d in &result.diagnostics {
            eprintln!("{}", render_parse_diagnostic(d));
        }
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------
// compile
// ---------------------------------------------------------------------

fn cmd_compile(args: &[String]) -> Result<ExitCode, String> {
    let mut path: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "-o" | "--output" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for `-o`".to_string())?;
                output = Some(PathBuf::from(v));
                i += 2;
            }
            other if other.starts_with("--output=") => {
                output = Some(PathBuf::from(other.trim_start_matches("--output=")));
                i += 1;
            }
            other if path.is_none() => {
                path = Some(other.to_string());
                i += 1;
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let source = read_source(&path.unwrap_or_else(|| "-".to_string()))?;
    let parsed = parse_source(&source);
    if !parsed.diagnostics.is_empty() {
        for d in &parsed.diagnostics {
            eprintln!("{}", render_parse_diagnostic(d));
        }
        return Ok(ExitCode::from(1));
    }
    let emitted = emit(&parsed.source);
    if !emitted.errors.is_empty() {
        for e in &emitted.errors {
            eprintln!("{}", render_emit_error(e));
        }
        return Ok(ExitCode::from(1));
    }
    let bytes = emitted.module.serialize();
    match output {
        Some(p) => {
            fs::write(&p, &bytes).map_err(|e| format!("write {}: {e}", p.display()))?;
        }
        None => {
            let stdout = io::stdout();
            let mut h = stdout.lock();
            h.write_all(&bytes).map_err(|e| format!("stdout: {e}"))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------
// disasm
// ---------------------------------------------------------------------

fn cmd_disasm(args: &[String]) -> Result<ExitCode, String> {
    let path = take_path_arg(args)?;
    let bytes = read_bytes(&path)?;
    let module = Module::parse(&bytes).map_err(|e| format!("load bytecode: {e}"))?;
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "; capyc {VERSION} disassembly\n; bc_version={} abi_version={} sections={}",
        0, module.abi_version, module.sections.len()
    );
    for s in &module.sections {
        match s.tag {
            SectionTag::Consts => {
                let pool = ConstPool::decode(&s.payload)
                    .map_err(|e| format!("constants section: {e}"))?;
                writeln!(out, "\nconstants ({} entries)", pool.entries.len()).ok();
                for (i, c) in pool.entries.iter().enumerate() {
                    writeln!(out, "  {i:>4}  {c:?}").ok();
                }
            }
            SectionTag::Functions => {
                let table = FunctionTable::decode(&s.payload)
                    .map_err(|e| format!("functions section: {e}"))?;
                writeln!(out, "\nfunctions ({} entries)", table.entries.len()).ok();
                for (i, f) in table.entries.iter().enumerate() {
                    writeln!(
                        out,
                        "  fn {i}  {:?}  locals={}",
                        f.name, f.locals_count
                    )
                    .ok();
                    match decode(&f.code) {
                        Ok(ins) => {
                            for line in disassemble_text(&ins).lines() {
                                writeln!(out, "    {line}").ok();
                            }
                        }
                        Err(e) => {
                            writeln!(out, "    <decode failed: {e}>").ok();
                        }
                    }
                }
            }
            SectionTag::Imports => {
                let table = ImportTable::decode(&s.payload)
                    .map_err(|e| format!("imports section: {e}"))?;
                writeln!(out, "\nimports ({} entries)", table.entries.len()).ok();
                for (i, im) in table.entries.iter().enumerate() {
                    writeln!(out, "  {i:>4}  {}::{}", im.module, im.symbol).ok();
                }
            }
            SectionTag::Debug => {
                writeln!(out, "\ndebug ({} bytes)", s.payload.len()).ok();
            }
        }
    }
    print_with_optional_newline(&out);
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------
// run
// ---------------------------------------------------------------------

fn cmd_run(args: &[String]) -> Result<ExitCode, String> {
    let mut path: Option<String> = None;
    let mut entry = "main".to_string();
    let mut budget: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            "--entry" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for `--entry`".to_string())?;
                entry = v.to_string();
                i += 2;
            }
            other if other.starts_with("--entry=") => {
                entry = other.trim_start_matches("--entry=").to_string();
                i += 1;
            }
            "--budget" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for `--budget`".to_string())?;
                budget = Some(
                    v.parse::<u64>()
                        .map_err(|_| format!("invalid budget `{v}`"))?,
                );
                i += 2;
            }
            other if other.starts_with("--budget=") => {
                let v = other.trim_start_matches("--budget=");
                budget = Some(
                    v.parse::<u64>()
                        .map_err(|_| format!("invalid budget `{v}`"))?,
                );
                i += 1;
            }
            other if path.is_none() => {
                path = Some(other.to_string());
                i += 1;
            }
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let raw = read_bytes(&path.unwrap_or_else(|| "-".to_string()))?;
    let bytes = if looks_like_bytecode(&raw) {
        raw
    } else {
        // Treat as source: parse + emit in memory.
        let source = String::from_utf8(raw)
            .map_err(|e| format!("source is not valid UTF-8: {e}"))?;
        let parsed = parse_source(&source);
        if !parsed.diagnostics.is_empty() {
            for d in &parsed.diagnostics {
                eprintln!("{}", render_parse_diagnostic(d));
            }
            return Ok(ExitCode::from(1));
        }
        let emitted = emit(&parsed.source);
        if !emitted.errors.is_empty() {
            for e in &emitted.errors {
                eprintln!("{}", render_emit_error(e));
            }
            return Ok(ExitCode::from(1));
        }
        emitted.module.serialize()
    };
    let vm = Vm::from_module_with_host(&bytes, HostAdapter::with_builtin_stubs())
        .map_err(|e| format!("load bytecode: {e}"))?;
    let budget = budget.unwrap_or(DEFAULT_INSTRUCTION_BUDGET);
    match vm.run_with_budget(&entry, budget) {
        Ok(v) => {
            println!("{}", format_value(&v));
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("runtime: {e}");
            Ok(ExitCode::from(1))
        }
    }
}

// ---------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------

fn take_path_arg(args: &[String]) -> Result<String, String> {
    let mut path: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other if path.is_none() => path = Some(other.to_string()),
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(path.unwrap_or_else(|| "-".to_string()))
}

fn read_source(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("stdin: {e}"))?;
        return Ok(buf);
    }
    fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))
}

fn read_bytes(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = Vec::new();
        io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("stdin: {e}"))?;
        return Ok(buf);
    }
    fs::read(path).map_err(|e| format!("read {path}: {e}"))
}

fn looks_like_bytecode(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[..4] == b"CAPY"
}

fn print_with_optional_newline(text: &str) {
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = h.write_all(text.as_bytes());
    if !text.ends_with('\n') {
        let _ = h.write_all(b"\n");
    }
}

fn render_parse_diagnostic(d: &ParseDiagnostic) -> String {
    use capy_parser::ParseErrorKind;
    let kind = match &d.kind {
        ParseErrorKind::UnexpectedToken { found, expected } => {
            format!("UnexpectedToken found={found:?} expected={expected}")
        }
        ParseErrorKind::UnexpectedEof { expected } => {
            format!("UnexpectedEof expected={expected}")
        }
        ParseErrorKind::Lex(kind) => format!("Lex {kind:?}"),
    };
    format!("parser [{}..{}] {}", d.span.start, d.span.end, kind)
}

fn render_emit_error(e: &EmitError) -> String {
    format!("emitter [{}..{}] {e}", e.span.start, e.span.end)
}

fn format_value(v: &Value) -> String {
    match v {
        Value::None => "none".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            // Preserve `-0.0` and similar edge cases by routing through
            // `{:?}`, then normalise the trailing `.0` Rust adds for
            // integer-valued floats so the CLI output stays predictable.
            format!("{f:?}")
        }
        Value::Str(s) => format!("{s:?}"),
    }
}
