//! VM diagnostics.
//!
//! Codes are part of the `capy-lang-host` v0 ABI. Adding a code is
//! additive within v0; renaming or removing one is a breaking change
//! and must be reflected in the cross-repo compatibility matrix.

#![forbid(unsafe_code)]

use std::fmt;

/// Stack popped while empty.
pub const V_STACK_UNDERFLOW: &str = "V0001";
/// `LoadLocal`/`StoreLocal` referenced a slot beyond the function's
/// declared `locals_count`.
pub const V_LOCAL_OUT_OF_BOUNDS: &str = "V0002";
/// `LoadConst` referenced an index outside the module's constant pool.
pub const V_CONST_OUT_OF_BOUNDS: &str = "V0003";
/// `Jump`/`JumpIfFalse` computed a target byte offset that does not
/// land on a known instruction boundary inside the current function.
pub const V_JUMP_OUT_OF_BOUNDS: &str = "V0004";
/// An opcode received operands of incompatible types.
pub const V_TYPE_MISMATCH: &str = "V0005";
/// Integer division or modulo by zero.
pub const V_DIVISION_BY_ZERO: &str = "V0006";
/// The instruction budget hit zero before the function returned.
pub const V_BUDGET_EXHAUSTED: &str = "V0007";
/// `Vm::run` requested a function name that the module does not export.
pub const V_UNKNOWN_FUNCTION: &str = "V0008";
/// The module bytes failed to load (header, sections, typed payloads
/// or instruction stream were malformed).
pub const V_MALFORMED_MODULE: &str = "V0009";
/// `JumpIfFalse` (or another bool-only opcode) received a non-bool
/// operand.
pub const V_EXPECTED_BOOL: &str = "V0010";
/// Recursive (or deeply nested) `Call` exceeded the VM's call-depth
/// limit. Deterministic fail-closed; the limit is documented in the
/// VM crate root and currently set to 256 frames.
pub const V_CALL_STACK_OVERFLOW: &str = "V0011";
/// `Call` referenced a function index that is out of bounds for the
/// module's function table.
pub const V_UNKNOWN_FUNCTION_INDEX: &str = "V0012";
/// `Call` requested an `argc` larger than the callee's declared
/// `locals_count`. The arity contract is enforced at runtime so a
/// malformed module cannot silently corrupt the callee's local frame.
pub const V_CALL_ARITY_MISMATCH: &str = "V0013";
/// `HostCall` referenced an `import_idx` that is out of bounds for the
/// module's `ImportTable`.
pub const V_UNKNOWN_HOST_IMPORT: &str = "V0014";
/// `HostCall` referenced an import for which the VM's host adapter has
/// no registered handler. Deterministic fail-closed: the script
/// terminates; the host process is unaffected.
pub const V_UNRESOLVED_HOST_IMPORT: &str = "V0015";
/// The registered host function rejected the call. The wrapped
/// `reason` is a short, host-supplied static string; sensitive payloads
/// must be redacted before reaching this surface (`docs/integration.md`
/// "host call privacy" rule).
pub const V_HOST_CALL_FAILED: &str = "V0016";

/// Errors that the VM produces during loading or execution. All
/// variants are fail-closed: the VM never panics on malformed bytecode
/// or runtime mismatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    StackUnderflow {
        pc: u32,
    },
    LocalOutOfBounds {
        pc: u32,
        index: u32,
        locals_count: u32,
    },
    ConstOutOfBounds {
        pc: u32,
        index: u32,
        pool_len: u32,
    },
    JumpOutOfBounds {
        pc: u32,
        target: i64,
    },
    TypeMismatch {
        pc: u32,
        op: &'static str,
        expected: &'static str,
        found: &'static str,
    },
    DivisionByZero {
        pc: u32,
    },
    BudgetExhausted {
        budget: u64,
    },
    UnknownFunction {
        name: String,
    },
    MalformedModule {
        reason: &'static str,
        code: &'static str,
    },
    ExpectedBool {
        pc: u32,
        found: &'static str,
    },
    CallStackOverflow {
        pc: u32,
        depth: usize,
    },
    UnknownFunctionIndex {
        pc: u32,
        index: u32,
        table_len: u32,
    },
    CallArityMismatch {
        pc: u32,
        callee: String,
        argc: u32,
        locals_count: u32,
    },
    UnknownHostImport {
        pc: u32,
        index: u32,
        table_len: u32,
    },
    UnresolvedHostImport {
        pc: u32,
        module: String,
        symbol: String,
    },
    HostCallFailed {
        pc: u32,
        module: String,
        symbol: String,
        reason: &'static str,
    },
}

impl VmError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StackUnderflow { .. } => V_STACK_UNDERFLOW,
            Self::LocalOutOfBounds { .. } => V_LOCAL_OUT_OF_BOUNDS,
            Self::ConstOutOfBounds { .. } => V_CONST_OUT_OF_BOUNDS,
            Self::JumpOutOfBounds { .. } => V_JUMP_OUT_OF_BOUNDS,
            Self::TypeMismatch { .. } => V_TYPE_MISMATCH,
            Self::DivisionByZero { .. } => V_DIVISION_BY_ZERO,
            Self::BudgetExhausted { .. } => V_BUDGET_EXHAUSTED,
            Self::UnknownFunction { .. } => V_UNKNOWN_FUNCTION,
            Self::MalformedModule { .. } => V_MALFORMED_MODULE,
            Self::ExpectedBool { .. } => V_EXPECTED_BOOL,
            Self::CallStackOverflow { .. } => V_CALL_STACK_OVERFLOW,
            Self::UnknownFunctionIndex { .. } => V_UNKNOWN_FUNCTION_INDEX,
            Self::CallArityMismatch { .. } => V_CALL_ARITY_MISMATCH,
            Self::UnknownHostImport { .. } => V_UNKNOWN_HOST_IMPORT,
            Self::UnresolvedHostImport { .. } => V_UNRESOLVED_HOST_IMPORT,
            Self::HostCallFailed { .. } => V_HOST_CALL_FAILED,
        }
    }

    /// Bytecode program-counter at which the error was observed, when
    /// the variant carries one.
    ///
    /// Variants tied to a specific instruction (every runtime trap
    /// except `BudgetExhausted`, `UnknownFunction` and
    /// `MalformedModule`, which are observed outside any executing
    /// frame) return `Some(pc)`. Downstream tooling can combine this
    /// with a `DebugInfo` lookup to resolve `pc` → source span — see
    /// `capy_diagnostics::bridge::from_vm_with_debug`.
    #[must_use]
    pub const fn pc(&self) -> Option<u32> {
        match self {
            Self::StackUnderflow { pc }
            | Self::LocalOutOfBounds { pc, .. }
            | Self::ConstOutOfBounds { pc, .. }
            | Self::JumpOutOfBounds { pc, .. }
            | Self::TypeMismatch { pc, .. }
            | Self::DivisionByZero { pc }
            | Self::ExpectedBool { pc, .. }
            | Self::CallStackOverflow { pc, .. }
            | Self::UnknownFunctionIndex { pc, .. }
            | Self::CallArityMismatch { pc, .. }
            | Self::UnknownHostImport { pc, .. }
            | Self::UnresolvedHostImport { pc, .. }
            | Self::HostCallFailed { pc, .. } => Some(*pc),
            Self::BudgetExhausted { .. }
            | Self::UnknownFunction { .. }
            | Self::MalformedModule { .. } => None,
        }
    }
}

/// Human-readable rendering for [`VmError`].
///
/// The output is deterministic and includes the stable diagnostic code
/// in square brackets so downstream tooling (the `capyc` CLI, the
/// planned `capy-diagnostics` bridge, and the eventual CapyOS adapter)
/// can route on the code while still presenting a readable line. The
/// format is **stable across patch releases within v0**; CapyOS-side
/// snapshot tests should match against the code, not the prose.
///
/// Privacy: every field already rendered here came in over the v0 ABI
/// (either a fixed `&'static str`, a bounded numeric index, or a
/// program-declared symbol name). No host paths, environment
/// variables or wall-clock values cross this surface — `HostCallFailed`
/// in particular carries only the handler's static `reason` string.
impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::StackUnderflow { pc } => {
                write!(f, "[{code}] stack underflow at pc=0x{pc:04x}")
            }
            Self::LocalOutOfBounds {
                pc,
                index,
                locals_count,
            } => write!(
                f,
                "[{code}] local slot {index} out of bounds (locals_count={locals_count}) at pc=0x{pc:04x}"
            ),
            Self::ConstOutOfBounds {
                pc,
                index,
                pool_len,
            } => write!(
                f,
                "[{code}] constant index {index} out of bounds (pool_len={pool_len}) at pc=0x{pc:04x}"
            ),
            Self::JumpOutOfBounds { pc, target } => write!(
                f,
                "[{code}] jump target {target} does not land on an instruction boundary at pc=0x{pc:04x}"
            ),
            Self::TypeMismatch {
                pc,
                op,
                expected,
                found,
            } => write!(
                f,
                "[{code}] type mismatch on `{op}`: expected {expected}, found {found} at pc=0x{pc:04x}"
            ),
            Self::DivisionByZero { pc } => {
                write!(f, "[{code}] division by zero at pc=0x{pc:04x}")
            }
            Self::BudgetExhausted { budget } => write!(
                f,
                "[{code}] instruction budget exhausted (budget={budget})"
            ),
            Self::UnknownFunction { name } => {
                write!(f, "[{code}] unknown function `{name}`")
            }
            Self::MalformedModule {
                reason,
                code: inner,
            } => write!(f, "[{code}] malformed module: {reason} ({inner})"),
            Self::ExpectedBool { pc, found } => write!(
                f,
                "[{code}] expected Bool, found {found} at pc=0x{pc:04x}"
            ),
            Self::CallStackOverflow { pc, depth } => write!(
                f,
                "[{code}] call stack overflow (depth={depth}) at pc=0x{pc:04x}"
            ),
            Self::UnknownFunctionIndex {
                pc,
                index,
                table_len,
            } => write!(
                f,
                "[{code}] unknown function index {index} (table_len={table_len}) at pc=0x{pc:04x}"
            ),
            Self::CallArityMismatch {
                pc,
                callee,
                argc,
                locals_count,
            } => write!(
                f,
                "[{code}] call to `{callee}` passes argc={argc} but callee declares locals_count={locals_count} at pc=0x{pc:04x}"
            ),
            Self::UnknownHostImport {
                pc,
                index,
                table_len,
            } => write!(
                f,
                "[{code}] unknown host import index {index} (table_len={table_len}) at pc=0x{pc:04x}"
            ),
            Self::UnresolvedHostImport {
                pc,
                module,
                symbol,
            } => write!(
                f,
                "[{code}] host adapter has no handler for `{module}::{symbol}` at pc=0x{pc:04x}"
            ),
            Self::HostCallFailed {
                pc,
                module,
                symbol,
                reason,
            } => write!(
                f,
                "[{code}] host call `{module}::{symbol}` rejected: {reason} at pc=0x{pc:04x}"
            ),
        }
    }
}

impl std::error::Error for VmError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_stable_code_and_pc() {
        let e = VmError::StackUnderflow { pc: 0x10 };
        let s = format!("{e}");
        assert!(s.starts_with("[V0001]"), "got {s:?}");
        assert!(s.contains("pc=0x0010"), "got {s:?}");
    }

    #[test]
    fn display_division_by_zero_is_redacted() {
        // No host paths, no environment, no wall-clock content.
        let e = VmError::DivisionByZero { pc: 7 };
        assert_eq!(format!("{e}"), "[V0006] division by zero at pc=0x0007");
    }

    #[test]
    fn display_host_call_failed_includes_reason_verbatim() {
        let e = VmError::HostCallFailed {
            pc: 12,
            module: "log".to_string(),
            symbol: "info".to_string(),
            reason: "log::info expects a Str argument",
        };
        let s = format!("{e}");
        assert!(s.starts_with("[V0016] host call `log::info` rejected: "), "got {s:?}");
        assert!(s.ends_with("at pc=0x000c"), "got {s:?}");
    }

    #[test]
    fn display_malformed_module_threads_inner_bytecode_code() {
        let e = VmError::MalformedModule {
            reason: "header magic mismatch",
            code: "B0001",
        };
        let s = format!("{e}");
        assert!(s.starts_with("[V0009] malformed module: "), "got {s:?}");
        assert!(s.contains("(B0001)"), "got {s:?}");
    }

    #[test]
    fn display_is_deterministic_under_repeat() {
        // Same inputs → identical text bytes.
        let e = VmError::CallArityMismatch {
            pc: 0xABCD,
            callee: "f".to_string(),
            argc: 3,
            locals_count: 1,
        };
        assert_eq!(format!("{e}"), format!("{e}"));
    }

    #[test]
    fn every_variant_displays_with_its_code_prefix() {
        // Catches a future variant that forgets to thread `code` into
        // its Display arm.
        let samples: Vec<(VmError, &'static str)> = vec![
            (VmError::StackUnderflow { pc: 0 }, V_STACK_UNDERFLOW),
            (
                VmError::LocalOutOfBounds {
                    pc: 0,
                    index: 0,
                    locals_count: 0,
                },
                V_LOCAL_OUT_OF_BOUNDS,
            ),
            (
                VmError::ConstOutOfBounds {
                    pc: 0,
                    index: 0,
                    pool_len: 0,
                },
                V_CONST_OUT_OF_BOUNDS,
            ),
            (
                VmError::JumpOutOfBounds { pc: 0, target: 0 },
                V_JUMP_OUT_OF_BOUNDS,
            ),
            (
                VmError::TypeMismatch {
                    pc: 0,
                    op: "x",
                    expected: "e",
                    found: "f",
                },
                V_TYPE_MISMATCH,
            ),
            (VmError::DivisionByZero { pc: 0 }, V_DIVISION_BY_ZERO),
            (VmError::BudgetExhausted { budget: 0 }, V_BUDGET_EXHAUSTED),
            (
                VmError::UnknownFunction {
                    name: "x".to_string(),
                },
                V_UNKNOWN_FUNCTION,
            ),
            (
                VmError::MalformedModule {
                    reason: "r",
                    code: "B0001",
                },
                V_MALFORMED_MODULE,
            ),
            (
                VmError::ExpectedBool { pc: 0, found: "f" },
                V_EXPECTED_BOOL,
            ),
            (
                VmError::CallStackOverflow { pc: 0, depth: 1 },
                V_CALL_STACK_OVERFLOW,
            ),
            (
                VmError::UnknownFunctionIndex {
                    pc: 0,
                    index: 0,
                    table_len: 0,
                },
                V_UNKNOWN_FUNCTION_INDEX,
            ),
            (
                VmError::CallArityMismatch {
                    pc: 0,
                    callee: "f".to_string(),
                    argc: 0,
                    locals_count: 0,
                },
                V_CALL_ARITY_MISMATCH,
            ),
            (
                VmError::UnknownHostImport {
                    pc: 0,
                    index: 0,
                    table_len: 0,
                },
                V_UNKNOWN_HOST_IMPORT,
            ),
            (
                VmError::UnresolvedHostImport {
                    pc: 0,
                    module: "m".to_string(),
                    symbol: "s".to_string(),
                },
                V_UNRESOLVED_HOST_IMPORT,
            ),
            (
                VmError::HostCallFailed {
                    pc: 0,
                    module: "m".to_string(),
                    symbol: "s".to_string(),
                    reason: "r",
                },
                V_HOST_CALL_FAILED,
            ),
        ];
        for (err, code) in samples {
            let s = format!("{err}");
            assert!(
                s.starts_with(&format!("[{code}]")),
                "variant {err:?} does not prefix with its code {code}: {s:?}"
            );
            assert_eq!(err.code(), code);
        }
    }
}
