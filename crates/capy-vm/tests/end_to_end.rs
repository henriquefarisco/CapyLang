//! End-to-end execution tests: source → AST → bytecode → VM → value.
//!
//! Exercises the full CapyLang pipeline through `capy-parser` +
//! `capy-emitter` + `capy-vm`. This is the first slice where a string
//! of source code can be evaluated to a runtime [`Value`].

use capy_emitter::emit;
use capy_parser::parse_source;
use capy_vm::{HostAdapter, Value, Vm, VmError};

fn run_source(src: &str) -> Result<Value, VmError> {
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "emit errors: {:?}", out.errors);
    let bytes = out.module.serialize();
    let vm = Vm::from_module(&bytes)?;
    vm.run("main")
}

#[test]
fn empty_function_returns_none() {
    assert_eq!(run_source("fn main() {}\n").unwrap(), Value::None);
}

#[test]
fn arithmetic_tail_returns_value() {
    assert_eq!(run_source("fn main() { 1 + 2 }\n").unwrap(), Value::Int(3));
    assert_eq!(
        run_source("fn main() { 10 - 3 * 2 }\n").unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        run_source("fn main() { (10 - 3) * 2 }\n").unwrap(),
        Value::Int(14)
    );
}

#[test]
fn boolean_comparison_returns_bool() {
    assert_eq!(
        run_source("fn main() { 1 == 1 }\n").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run_source("fn main() { 1 == 2 }\n").unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        run_source("fn main() { 1 != 2 }\n").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run_source("fn main() { 3 < 4 }\n").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run_source("fn main() { 4 <= 4 }\n").unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn let_and_use() {
    assert_eq!(
        run_source("fn main() { let x = 42; x }\n").unwrap(),
        Value::Int(42)
    );
    assert_eq!(
        run_source("fn main() { let x = 5; let y = 7; x + y }\n").unwrap(),
        Value::Int(12)
    );
}

#[test]
fn if_then_branch_executed() {
    assert_eq!(
        run_source("fn main() { if 1 == 1 { 10 } else { 20 } }\n").unwrap(),
        Value::Int(10)
    );
}

#[test]
fn if_else_branch_executed() {
    assert_eq!(
        run_source("fn main() { if 1 == 2 { 10 } else { 20 } }\n").unwrap(),
        Value::Int(20)
    );
}

#[test]
fn nested_if_picks_innermost_branch() {
    let src = "fn main() { \
                let x = 3; \
                if x == 1 { 100 } \
                else { if x == 3 { 300 } else { 999 } } \
              }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(300));
}

#[test]
fn explicit_return_short_circuits() {
    assert_eq!(
        run_source("fn main() { return 7; }\n").unwrap(),
        Value::Int(7)
    );
}

#[test]
fn early_return_inside_if() {
    let src = "fn main() { \
                if 1 == 1 { return 99; } \
                else { return 1; } \
              }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(99));
}

#[test]
fn unary_neg_and_not() {
    assert_eq!(run_source("fn main() { -42 }\n").unwrap(), Value::Int(-42));
    assert_eq!(
        run_source("fn main() { !(1 == 2) }\n").unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn float_arithmetic() {
    assert_eq!(
        run_source("fn main() { 1.5 + 2.5 }\n").unwrap(),
        Value::Float(4.0)
    );
}

#[test]
fn int_float_promotion() {
    // Mixing Int + Float promotes to Float.
    assert_eq!(
        run_source("fn main() { 1 + 2.5 }\n").unwrap(),
        Value::Float(3.5)
    );
}

#[test]
fn string_constant_loads() {
    assert_eq!(
        run_source("fn main() { \"hello\" }\n").unwrap(),
        Value::Str("hello".to_string())
    );
}

#[test]
fn string_with_escapes() {
    // \n escape decoded by the emitter, preserved by the VM.
    assert_eq!(
        run_source("fn main() { \"a\\nb\" }\n").unwrap(),
        Value::Str("a\nb".to_string())
    );
}

#[test]
fn division_by_zero_at_runtime_traps() {
    let src = "fn main() { let x = 0; 10 / x }\n";
    match run_source(src).unwrap_err() {
        VmError::DivisionByZero { .. } => {}
        other => panic!("unexpected: {other:?}"),
    }
}

// --- S5b.2: loops, break/continue, short-circuit boolean operators ---

#[test]
fn while_false_skips_body_and_returns_none() {
    assert_eq!(
        run_source("fn main() { while false { 1; } }\n").unwrap(),
        Value::None
    );
}

#[test]
fn loop_break_returns_value() {
    assert_eq!(
        run_source("fn main() { loop { break 42; } }\n").unwrap(),
        Value::Int(42)
    );
}

#[test]
fn loop_break_without_value_returns_none() {
    assert_eq!(
        run_source("fn main() { loop { break; } }\n").unwrap(),
        Value::None
    );
}

#[test]
fn break_inside_nested_if_in_loop() {
    let src = "fn main() { loop { if 1 == 1 { break 7; } else { break 0; } } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(7));
}

#[test]
fn short_circuit_and_returns_lhs_when_false() {
    assert_eq!(
        run_source("fn main() { false && true }\n").unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        run_source("fn main() { true && false }\n").unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        run_source("fn main() { true && true }\n").unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn short_circuit_or_returns_lhs_when_true() {
    assert_eq!(
        run_source("fn main() { true || false }\n").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run_source("fn main() { false || true }\n").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        run_source("fn main() { false || false }\n").unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn short_circuit_and_skips_rhs_evaluation() {
    // If `&&` did not short-circuit, the rhs `10 / 0 == 1` would trap
    // with DivisionByZero. A `false` lhs must skip it entirely.
    assert_eq!(
        run_source("fn main() { false && (10 / 0 == 1) }\n").unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn short_circuit_or_skips_rhs_evaluation() {
    assert_eq!(
        run_source("fn main() { true || (10 / 0 == 1) }\n").unwrap(),
        Value::Bool(true)
    );
}

// --- S5b.3: function calls + parameters ---

#[test]
fn call_with_two_params_returns_sum() {
    let src = "fn add(x: i32, y: i32) -> i32 { x + y }\nfn main() { add(2, 3) }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(5));
}

#[test]
fn call_with_no_params_returns_constant() {
    let src = "fn answer() { 42 }\nfn main() { answer() }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(42));
}

#[test]
fn nested_calls_compose_results() {
    let src = "\
        fn inc(x: i32) -> i32 { x + 1 }\n\
        fn double(x: i32) -> i32 { x + x }\n\
        fn main() { double(inc(3)) }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(8));
}

#[test]
fn forward_reference_is_callable() {
    // `main` is declared *before* `helper` in the source; the two-pass
    // emitter must still resolve the call.
    let src = "fn main() { helper(10) }\nfn helper(n: i32) -> i32 { n * n }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(100));
}

#[test]
fn recursive_call_terminates_on_base_case() {
    // Classical sum-down-to-zero: avoids ordering / iteration cliffs.
    //   sum(n) = if n == 0 { 0 } else { n + sum(n - 1) }
    let src = "\
        fn sum(n: i32) -> i32 { if n == 0 { 0 } else { n + sum(n - 1) } }\n\
        fn main() { sum(5) }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(15));
}

#[test]
fn recursion_beyond_max_depth_traps_call_stack_overflow() {
    // Unbounded recursion: the VM must trap deterministically rather
    // than blowing the native stack.
    let src = "fn loop_forever() { loop_forever() }\nfn main() { loop_forever() }\n";
    match run_source(src).unwrap_err() {
        VmError::CallStackOverflow { depth, .. } => {
            assert_eq!(depth, capy_vm::MAX_CALL_DEPTH);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn parameters_seed_locals_in_declaration_order() {
    // The first parameter is callee's locals[0]; verify by passing two
    // distinct values and returning the first one only.
    let src = "fn first(a: i32, b: i32) -> i32 { a }\nfn main() { first(11, 22) }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(11));
}

#[test]
fn local_let_after_params_extends_locals_window() {
    // Params occupy locals[0..N]; subsequent `let` bindings extend into
    // locals[N..]. Verify that both coexist correctly.
    let src = "\
        fn f(x: i32) -> i32 { let y = x + 1; let z = y + 1; x + y + z }\n\
        fn main() { f(10) }\n";
    // x=10, y=11, z=12, total=33.
    assert_eq!(run_source(src).unwrap(), Value::Int(33));
}

#[test]
fn deterministic_repeat_execution() {
    // Running the same compiled module twice produces the exact same
    // value (no hidden state, no time, no rng).
    let parsed = parse_source("fn main() { 1 + 2 + 3 }\n");
    let out = emit(&parsed.source);
    let bytes = out.module.serialize();
    let vm = Vm::from_module(&bytes).unwrap();
    let r1 = vm.run("main").unwrap();
    let r2 = vm.run("main").unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r1, Value::Int(6));
}

// --- S7: source → host_call → adapter -------------------------------

fn run_source_with_host(src: &str, host: HostAdapter) -> Result<Value, VmError> {
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "emit errors: {:?}", out.errors);
    let bytes = out.module.serialize();
    let vm = Vm::from_module_with_host(&bytes, host)?;
    vm.run("main")
}

#[test]
fn imported_call_runs_through_host_adapter() {
    // `import time::now; fn main() { now() }` lowers `now()` to
    // HostCall and the built-in stub returns Int(0) deterministically.
    let src = "import time::now;\nfn main() { now() }\n";
    let result = run_source_with_host(src, HostAdapter::with_builtin_stubs()).unwrap();
    assert_eq!(result, Value::Int(0));
}

#[test]
fn import_alias_lets_source_use_local_name() {
    // The host adapter only sees (module="log", symbol="info").
    let src = "import log::info as say;\nfn main() { say(\"hello\") }\n";
    let result = run_source_with_host(src, HostAdapter::with_builtin_stubs()).unwrap();
    assert_eq!(result, Value::None);
}

#[test]
fn unresolved_import_traps_deterministically_at_runtime() {
    // The source imports a symbol the host adapter does not register.
    // The verifier still accepts the module (HostCall is structurally
    // valid); execution traps with UnresolvedHostImport.
    let src = "import nope::missing;\nfn main() { missing() }\n";
    match run_source_with_host(src, HostAdapter::with_builtin_stubs()) {
        Err(VmError::UnresolvedHostImport { module, symbol, .. }) => {
            assert_eq!(module, "nope");
            assert_eq!(symbol, "missing");
        }
        other => panic!("expected UnresolvedHostImport, got {other:?}"),
    }
}

// --- S2.2b → emitter → VM: match lowering ---------------------------

#[test]
fn match_selects_first_matching_literal_arm() {
    // `match 1 { 0 => 100, 1 => 200, _ => 999 }` returns 200 because
    // arm 1 is the first whose literal pattern is equal to the
    // scrutinee.
    let src = "fn main() { match 1 { 0 => 100, 1 => 200, _ => 999 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(200));
}

#[test]
fn match_falls_through_to_wildcard_when_no_literal_matches() {
    let src = "fn main() { match 7 { 0 => 100, 1 => 200, _ => 999 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(999));
}

#[test]
fn match_ident_pattern_binds_scrutinee_into_arm_body() {
    // `n` is bound to the scrutinee (5) and used in the arm body.
    let src = "fn main() { match 5 { n => n + 1 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(6));
}

#[test]
fn match_guard_skips_arm_when_guard_false() {
    // First arm's pattern matches (`n` binds 3) but guard `n > 10`
    // fails; control flows to the wildcard arm.
    let src = "fn main() { match 3 { n if n > 10 => 1, _ => 0 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(0));
}

#[test]
fn match_guard_succeeds_when_guard_true() {
    let src = "fn main() { match 30 { n if n > 10 => 1, _ => 0 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(1));
}

#[test]
fn match_with_no_matching_arm_returns_none() {
    // First-cut lowering treats a non-exhaustive match as a
    // deterministic fall-through to `Value::None` so the VM stays
    // verifier-safe. Exhaustiveness checking is a future slice.
    let src = "fn main() { match 7 { 0 => 100, 1 => 200 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::None);
}

#[test]
fn match_as_let_initializer_round_trips() {
    // `let v = match c { ... };` then return `v` exercises the
    // match's +1 stack contract from inside a Let initializer.
    let src = "fn main() { let v = match 2 { 1 => 10, 2 => 20, _ => 0 }; v }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(20));
}

#[test]
fn match_inclusive_range_pattern_matches_inside_bounds() {
    // `0..=9` is inclusive on both endpoints.
    assert_eq!(
        run_source("fn main() { match 0 { 0..=9 => 1, _ => 0 } }\n").unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        run_source("fn main() { match 9 { 0..=9 => 1, _ => 0 } }\n").unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        run_source("fn main() { match 10 { 0..=9 => 1, _ => 0 } }\n").unwrap(),
        Value::Int(0)
    );
}

#[test]
fn match_exclusive_range_pattern_excludes_upper_bound() {
    // `0..9` excludes 9; matches 0..8 inclusive.
    assert_eq!(
        run_source("fn main() { match 8 { 0..9 => 1, _ => 0 } }\n").unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        run_source("fn main() { match 9 { 0..9 => 1, _ => 0 } }\n").unwrap(),
        Value::Int(0)
    );
}

#[test]
fn match_or_pattern_matches_any_alternative() {
    // Each of the three alts should select the same arm body.
    for scrut in [1, 2, 3] {
        let src = format!("fn main() {{ match {scrut} {{ 1 | 2 | 3 => 100, _ => 0 }} }}\n");
        assert_eq!(run_source(&src).unwrap(), Value::Int(100), "scrut={scrut}");
    }
    // A scrutinee not in the alt set falls through to the wildcard.
    assert_eq!(
        run_source("fn main() { match 7 { 1 | 2 | 3 => 100, _ => 0 } }\n").unwrap(),
        Value::Int(0)
    );
}

#[test]
fn match_or_pattern_combines_with_guard() {
    // The guard runs after one of the or-alts matches.
    let src = "\
fn main() { \
    match 2 { \
        1 | 2 | 3 if 1 > 0 => 100, \
        _ => 0, \
    } \
}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(100));
    let src_fail = "\
fn main() { \
    match 2 { \
        1 | 2 | 3 if 1 < 0 => 100, \
        _ => 0, \
    } \
}\n";
    assert_eq!(run_source(src_fail).unwrap(), Value::Int(0));
}

#[test]
fn match_range_pattern_combines_with_wildcard_fallback() {
    // `0..=9 => "low"`, `_ => "high"` exercises the verifier-safe
    // post-range fall-through to the next arm.
    let src = "\
fn main() { \
    match 42 { \
        0..=9 => 1, \
        _ => 99, \
    } \
}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(99));
}

#[test]
fn nested_match_is_verifier_safe() {
    // Nested matches each allocate their own scrutinee slot via
    // `alloc_unnamed_local`, so the verifier sees stack-discipline-
    // clean lowering and the inner result flows out as the outer
    // arm body's value.
    let src = "\
fn main() { \
    match 1 { \
        1 => match 2 { 2 => 42, _ => 0 }, \
        _ => 999, \
    } \
}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(42));
}
