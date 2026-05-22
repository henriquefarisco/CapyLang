//! CapyLang bytecode VM (slice S6).
//!
//! This crate executes a v0 [`Module`](capy_bytecode::Module) on a
//! deterministic stack machine. The VM is decoupled from CapyOS: it has
//! no kernel dependencies, no JIT, no syscalls, no host pointers and
//! no global state. Calls are budgeted: every executed instruction
//! decrements a counter, and exhausting the budget traps with a
//! deterministic [`VmError::BudgetExhausted`].
//!
//! # Determinism contract
//!
//! * Same input module + same entry-point + same budget ⇒ same
//!   [`Value`] or [`VmError`].
//! * Integer arithmetic is *wrapping* (no panics on overflow).
//! * Integer division/modulo by zero traps with [`VmError::DivisionByZero`].
//! * Float arithmetic follows IEEE-754; `NaN == NaN` is `false` (the
//!   standard `f64::PartialEq` behaviour).
//! * Arithmetic and comparison opcodes require matching types. The
//!   only cross-type promotion is `Int <-> Float` for numeric
//!   operators (`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Eq`, `Ne`, `Lt`,
//!   `Le`, `Gt`, `Ge`); mixing other categories traps with
//!   [`VmError::TypeMismatch`].
//! * `JumpIfFalse` requires a [`Value::Bool`]; other types trap with
//!   [`VmError::ExpectedBool`].
//! * `Call` enforces a deterministic depth limit
//!   [`MAX_CALL_DEPTH`](execute::MAX_CALL_DEPTH) (currently 256
//!   frames); recursion beyond it traps with
//!   [`VmError::CallStackOverflow`].
//! * `Call` enforces that the requested `argc` does not exceed the
//!   callee's declared `locals_count`; mismatch traps with
//!   [`VmError::CallArityMismatch`].
//!
//! # Example
//!
//! ```
//! use capy_emitter::emit;
//! use capy_parser::parse_source;
//! use capy_vm::{Value, Vm};
//!
//! let parsed = parse_source("fn main() { 1 + 2 }\n");
//! let out = emit(&parsed.source);
//! assert!(out.errors.is_empty());
//! let bytes = out.module.serialize();
//! let vm = Vm::from_module(&bytes).unwrap();
//! assert_eq!(vm.run("main").unwrap(), Value::Int(3));
//! ```

#![forbid(unsafe_code)]

mod error;
mod execute;
mod host;
mod value;

pub use error::{
    VmError, V_BUDGET_EXHAUSTED, V_CALL_ARITY_MISMATCH, V_CALL_STACK_OVERFLOW,
    V_CONST_OUT_OF_BOUNDS, V_DIVISION_BY_ZERO, V_EXPECTED_BOOL, V_HOST_CALL_FAILED,
    V_JUMP_OUT_OF_BOUNDS, V_LOCAL_OUT_OF_BOUNDS, V_MALFORMED_MODULE, V_STACK_UNDERFLOW,
    V_TYPE_MISMATCH, V_UNKNOWN_FUNCTION, V_UNKNOWN_FUNCTION_INDEX, V_UNKNOWN_HOST_IMPORT,
    V_UNRESOLVED_HOST_IMPORT,
};
pub use execute::{Vm, DEFAULT_INSTRUCTION_BUDGET, MAX_CALL_DEPTH};
pub use host::{HostAdapter, HostFn, HostResult};
pub use value::Value;
