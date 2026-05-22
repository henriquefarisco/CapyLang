//! End-to-end test: source → emitter (with DebugInfo) → VM → trap →
//! `bridge::from_vm_with_debug` resolves `pc` back to a source span.
//!
//! This is the first slice where a runtime trap can be rendered with
//! a source caret. The emitter's debug entries flow through the
//! `Debug` section in the module, the VM keeps it for the diagnostics
//! layer to consume, and the bridge looks up the trap's `pc` against
//! the resolved entries.

use capy_bytecode::{DebugInfo, Module, SectionTag};
use capy_diagnostics::{from_vm_with_debug, render};
use capy_emitter::emit;
use capy_lexer::Span;
use capy_parser::parse_source;
use capy_vm::{Vm, VmError};

fn compile(src: &str) -> (Module, Vec<u8>) {
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "emit errors: {:?}", out.errors);
    let bytes = out.module.serialize();
    (out.module, bytes)
}

fn debug_info(module: &Module) -> DebugInfo {
    let section = module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Debug)
        .expect("emitter shipped a Debug section");
    DebugInfo::decode(&section.payload).expect("DebugInfo::decode")
}

#[test]
fn division_by_zero_trap_resolves_to_div_span() {
    // The `/` operator span points at the binary expression; the trap
    // raised at the `div` opcode should resolve to a sub-span inside
    // the function body.
    let src = "fn main() { 1 / 0 }\n";
    let (module, bytes) = compile(src);
    let debug = debug_info(&module);

    let vm = Vm::from_module(&bytes).unwrap();
    let err = vm.run("main").unwrap_err();
    assert!(matches!(err, VmError::DivisionByZero { .. }));

    let diag = from_vm_with_debug(&err, &debug);
    // The resolved span must be inside the function body
    // `1 / 0` which sits between `{` and `}`.
    let open = src.find('{').unwrap() + 1;
    let close = src.rfind('}').unwrap();
    assert!(
        diag.primary.span.start >= open && diag.primary.span.end <= close,
        "resolved span {:?} not inside function body [{open}, {close})",
        diag.primary.span,
    );
    // And it must be non-empty so the renderer can produce a caret.
    assert!(diag.primary.span.start < diag.primary.span.end);
}

#[test]
fn rendered_runtime_diagnostic_contains_source_caret() {
    let src = "fn main() { 1 / 0 }\n";
    let (module, bytes) = compile(src);
    let debug = debug_info(&module);
    let vm = Vm::from_module(&bytes).unwrap();
    let err = vm.run("main").unwrap_err();

    let diag = from_vm_with_debug(&err, &debug);
    let rendered = render(&diag, src, "<runtime>");
    // Header contains the stable VM code.
    assert!(rendered.starts_with("error[V0006]:"), "got {rendered:?}");
    // The renderer prints a source line and a caret region. The
    // exact column depends on the resolved span, but the snippet
    // must include the original `1 / 0` substring.
    assert!(rendered.contains("1 / 0"), "got {rendered:?}");
}

#[test]
fn pcless_error_falls_back_to_origin_span_even_with_debug() {
    // `BudgetExhausted` has no pc, so the resolver cannot supply a
    // span. Pair it with a populated DebugInfo to confirm the bridge
    // still falls back gracefully to `Span::new(0, 0)`.
    let src = "fn main() { loop {} }\n";
    let (module, bytes) = compile(src);
    let debug = debug_info(&module);
    let vm = Vm::from_module(&bytes).unwrap();
    // Set a tiny budget to force BudgetExhausted before the loop
    // makes meaningful progress.
    let err = vm.run_with_budget("main", 3).unwrap_err();
    assert!(matches!(err, VmError::BudgetExhausted { .. }));

    let diag = from_vm_with_debug(&err, &debug);
    assert_eq!(diag.primary.span, Span::new(0, 0));
}
