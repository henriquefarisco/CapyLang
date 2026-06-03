//! CapyLang bytecode emitter (slice S5b).
//!
//! Lowers a parsed [`Source`](capy_ast::Source) into a v0
//! [`Module`](capy_bytecode::Module). The output is always a complete,
//! self-consistent module (the bytecode container header, body checksum
//! and section framing are computed by `capy-bytecode`); per-item
//! lowering failures are accumulated into [`EmitError`] entries and the
//! offending function is omitted from the output so partial emission is
//! still meaningful.
//!
//! S5b.1 covers literals, locals, paren, unary `-` / `!`, binary
//! arithmetic and comparison, blocks, `let`, expression statements,
//! `if`/`else`, `return` and the top-level `Item::Fn` and
//! `Item::Import` forms. S5b.2 adds `while` / `loop` / `break` /
//! `continue` (using the existing v0 `Jump` / `JumpIfFalse` opcodes
//! plus `Pop` / `LoadNone` for the join point) and short-circuit
//! `&&` / `||`. S5b.3 adds function parameters (registered as the
//! first locals of each function) and direct calls to top-level
//! functions via the new v0 `Call (fn_idx, argc)` opcode, with a
//! two-pass module-level resolution so forward and backward calls
//! both work and per-function emission failures keep all other
//! indices stable. S7 wires `Item::Import` declarations into the
//! same resolution map: a call whose callee name resolves to an
//! imported `module::symbol` lowers to `HostCall (import_idx, argc)`
//! instead of `Call`, with local `fn` items shadowing imports of
//! the same name (Rust `use` precedence).
//!
//! # Example
//!
//! ```
//! use capy_emitter::emit;
//! use capy_parser::parse_source;
//! use capy_bytecode::{decode, FunctionTable, Instruction, SectionTag};
//!
//! let parsed = parse_source("fn add() { 1 + 2 }\n");
//! let out = emit(&parsed.source);
//! assert!(out.errors.is_empty());
//! let functions_section = out
//!     .module
//!     .sections
//!     .iter()
//!     .find(|s| s.tag == SectionTag::Functions)
//!     .unwrap();
//! let table = FunctionTable::decode(&functions_section.payload).unwrap();
//! let stream = decode(&table.entries[0].code).unwrap();
//! assert_eq!(
//!     stream,
//!     vec![
//!         Instruction::LoadConst(0),
//!         Instruction::LoadConst(1),
//!         Instruction::Add,
//!         Instruction::Return,
//!     ]
//! );
//! ```

#![forbid(unsafe_code)]

mod emit;
mod error;

pub use emit::{emit, EmitOutput};
pub use error::{
    EmitError, EmitErrorKind, E_BREAK_OUTSIDE_LOOP, E_CONTINUE_OUTSIDE_LOOP, E_DUPLICATE_FUNCTION,
    E_DUPLICATE_IMPORT, E_DUPLICATE_LOCAL, E_FLOAT_PARSE, E_INTEGER_PARSE, E_NESTED_ITEM,
    E_PARSE_ERROR_IN_EXPR, E_STRING_PARSE, E_TOO_MANY_ARGUMENTS, E_TOP_LEVEL_MUST_BE_ITEM,
    E_UNKNOWN_FUNCTION, E_UNKNOWN_LOCAL, E_UNSUPPORTED_BINARY, E_UNSUPPORTED_CALLEE,
    E_UNSUPPORTED_EXPR, E_UNSUPPORTED_FEATURE, E_UNSUPPORTED_ITEM, E_UNSUPPORTED_UNARY,
};
