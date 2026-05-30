//! Host bridge for the v0 VM (S7 sketch).
//!
//! The VM communicates with the surrounding process exclusively through
//! a [`HostAdapter`]: a small registry mapping `(module, symbol)` pairs
//! (as listed in the bytecode's `Imports` section) to deterministic
//! Rust handlers. Bytecode invokes a handler via the `HostCall` opcode
//! (`0x82`) — never via raw pointers, syscalls or kernel handles.
//!
//! ## Invariants
//!
//! - **Opaque handles.** Handlers receive a borrowed `&[Value]` slice
//!   and return a single `Value`. No raw process memory crosses the
//!   boundary; the VM owns every byte the handler observes.
//! - **Deterministic.** The built-in stubs (`time::now`, `log::info`)
//!   never read wall-clock time, process environment or random sources
//!   inside this crate. A real CapyOS adapter substitutes deterministic
//!   sources (frame counter, monotonic budget) per
//!   `docs/integration.md` "host call privacy" and "frame budget"
//!   sections.
//! - **Fail-closed.** A missing import, an arity mismatch or a
//!   handler-reported error trap the script via [`VmError`]; the host
//!   process is never affected.
//! - **No unsafe.** Per the workspace decoupling discipline, this
//!   module forbids `unsafe`.
//!
//! The adapter is intentionally tiny in S7: it covers exactly what the
//! verifier + VM execute paths need to exercise. The host ABI module
//! surface (`time`, `log`, `fs`, `config`, `gfx2d`, `input`, `metrics`)
//! is still owned by CapyOS and lands at Etapa 15.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use crate::value::Value;

/// Result type returned by host functions.
///
/// `Ok(value)` is pushed back onto the operand stack. `Err(reason)` is
/// surfaced as [`crate::VmError::HostCallFailed`] with the reason
/// preserved verbatim. The reason must be `&'static str` to keep the
/// error type `'static`-clean and to enforce that handlers do not leak
/// dynamic, potentially sensitive content into diagnostics by default.
pub type HostResult = Result<Value, &'static str>;

/// Function-pointer signature for a host handler.
///
/// Handlers receive their arguments in the same left-to-right order
/// they were pushed by the bytecode (slot `0` is the first argument).
pub type HostFn = fn(args: &[Value]) -> HostResult;

/// Registry of `(module, symbol)` → handler.
///
/// `HostAdapter::default()` returns an empty adapter. Use
/// [`HostAdapter::with_builtin_stubs`] to obtain a tiny set of
/// deterministic stubs intended for tests and the `capyc` CLI; CapyOS
/// will provide its own adapter at Etapa 15.
#[derive(Debug, Default, Clone)]
pub struct HostAdapter {
    entries: HashMap<(String, String), HostFn>,
}

impl HostAdapter {
    /// Builds an empty adapter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Registers `handler` under `(module, symbol)`. Replaces any
    /// previously registered entry for the same key.
    pub fn register(&mut self, module: &str, symbol: &str, handler: HostFn) {
        self.entries
            .insert((module.to_string(), symbol.to_string()), handler);
    }

    /// Returns the handler registered for `(module, symbol)`, if any.
    #[must_use]
    pub fn lookup(&self, module: &str, symbol: &str) -> Option<HostFn> {
        self.entries
            .get(&(module.to_string(), symbol.to_string()))
            .copied()
    }

    /// Convenience: returns an adapter pre-populated with the small
    /// deterministic stub set used by VM tests and the `capyc run`
    /// subcommand. The stubs do **not** read wall-clock time or
    /// process environment; CapyOS replaces them at Etapa 15.
    #[must_use]
    pub fn with_builtin_stubs() -> Self {
        let mut a = Self::new();
        a.register("time", "now", stub_time_now);
        a.register("log", "info", stub_log_info);
        a
    }
}

/// `time::now` — returns a fixed `Int(0)` so the VM trace remains
/// reproducible. CapyOS replaces this with a monotonic counter wired
/// to its frame scheduler.
fn stub_time_now(args: &[Value]) -> HostResult {
    if !args.is_empty() {
        return Err("time::now expects no arguments");
    }
    Ok(Value::Int(0))
}

/// `log::info` — accepts a single `Str` argument and returns
/// `Value::None`. The stub does not actually print; CapyOS adapters
/// route the payload through the kernel log with redaction.
fn stub_log_info(args: &[Value]) -> HostResult {
    match args {
        [Value::Str(_)] => Ok(Value::None),
        [_] => Err("log::info expects a Str argument"),
        _ => Err("log::info expects exactly one argument"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_adapter_returns_none_on_lookup() {
        let a = HostAdapter::new();
        assert!(a.lookup("time", "now").is_none());
    }

    #[test]
    fn register_then_lookup_roundtrips() {
        let mut a = HostAdapter::new();
        a.register("time", "now", stub_time_now);
        assert!(a.lookup("time", "now").is_some());
        assert!(a.lookup("time", "missing").is_none());
    }

    #[test]
    fn builtin_stubs_cover_time_and_log() {
        let a = HostAdapter::with_builtin_stubs();
        let now = a.lookup("time", "now").expect("time::now registered");
        assert_eq!(now(&[]).unwrap(), Value::Int(0));
        let info = a.lookup("log", "info").expect("log::info registered");
        assert_eq!(info(&[Value::Str("hello".into())]).unwrap(), Value::None);
    }

    #[test]
    fn time_now_rejects_arguments() {
        assert!(stub_time_now(&[Value::Int(0)]).is_err());
    }

    #[test]
    fn log_info_rejects_non_string() {
        assert!(stub_log_info(&[Value::Int(0)]).is_err());
        assert!(stub_log_info(&[]).is_err());
        assert!(stub_log_info(&[Value::Str("a".into()), Value::Str("b".into())]).is_err());
    }
}
