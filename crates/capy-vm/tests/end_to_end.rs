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
fn assignment_mutates_local() {
    assert_eq!(
        run_source("fn main() { let x = 1; x = 2; x }\n").unwrap(),
        Value::Int(2)
    );
}

#[test]
fn compound_assignment_accumulates() {
    assert_eq!(
        run_source("fn main() { let x = 10; x += 5; x }\n").unwrap(),
        Value::Int(15)
    );
}

#[test]
fn while_loop_with_mutation_sums_range() {
    // The headline win of S2.4: an imperative counting loop is now
    // expressible because locals can be reassigned. Sums 1..=5 = 15.
    let src = "fn main() {\n    let total = 0;\n    let i = 1;\n    while i <= 5 {\n        total = total + i;\n        i = i + 1;\n    }\n    total\n}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(15));
}

#[test]
fn bitwise_operators_evaluate() {
    assert_eq!(run_source("fn main() { 6 & 3 }\n").unwrap(), Value::Int(2));
    assert_eq!(run_source("fn main() { 6 | 1 }\n").unwrap(), Value::Int(7));
    assert_eq!(run_source("fn main() { 6 ^ 3 }\n").unwrap(), Value::Int(5));
    assert_eq!(
        run_source("fn main() { 1 << 4 }\n").unwrap(),
        Value::Int(16)
    );
    assert_eq!(
        run_source("fn main() { 32 >> 2 }\n").unwrap(),
        Value::Int(8)
    );
}

#[test]
fn bitwise_not_complements_int() {
    // Two's complement: ~0 == -1, ~5 == -6.
    assert_eq!(run_source("fn main() { ~0 }\n").unwrap(), Value::Int(-1));
    assert_eq!(run_source("fn main() { ~5 }\n").unwrap(), Value::Int(-6));
}

#[test]
fn bitwise_on_non_int_traps_type_mismatch() {
    match run_source("fn main() { true & false }\n") {
        Err(VmError::TypeMismatch { .. }) => {}
        other => panic!("expected TypeMismatch, got {other:?}"),
    }
}

#[test]
fn string_concatenation_with_plus() {
    assert_eq!(
        run_source("fn main() { \"foo\" + \"bar\" }\n").unwrap(),
        Value::Str("foobar".to_string())
    );
}

#[test]
fn adding_int_and_string_traps() {
    match run_source("fn main() { 1 + \"x\" }\n") {
        Err(VmError::TypeMismatch { .. }) => {}
        other => panic!("expected TypeMismatch, got {other:?}"),
    }
}

#[test]
fn for_loop_inclusive_range_sums() {
    // for i in 1..=5 { total = total + i; }  → 1+2+3+4+5 = 15
    let src = "fn main() {\n    let total = 0;\n    for i in 1..=5 {\n        total = total + i;\n    }\n    total\n}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(15));
}

#[test]
fn for_loop_exclusive_range_sums() {
    // for i in 0..5 { total = total + i; }  → 0+1+2+3+4 = 10
    let src = "fn main() {\n    let total = 0;\n    for i in 0..5 {\n        total = total + i;\n    }\n    total\n}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(10));
}

#[test]
fn for_loop_with_continue_still_advances() {
    // `continue` must jump to the increment, not the header, so the loop
    // terminates. Counts iterations 0..5 → 5.
    let src = "fn main() {\n    let n = 0;\n    for i in 0..5 {\n        n = n + 1;\n        continue;\n    }\n    n\n}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(5));
}

#[test]
fn array_literal_and_index() {
    assert_eq!(
        run_source("fn main() { let a = [10, 20, 30]; a[1] }\n").unwrap(),
        Value::Int(20)
    );
}

#[test]
fn array_element_assignment_mutates_in_place() {
    // Reference semantics: writing through one binding is visible when the
    // same local is read back.
    assert_eq!(
        run_source("fn main() { let a = [1, 2, 3]; a[0] = 9; a[0] }\n").unwrap(),
        Value::Int(9)
    );
}

#[test]
fn array_out_of_bounds_traps() {
    match run_source("fn main() { let a = [1, 2]; a[5] }\n") {
        Err(VmError::IndexOutOfBounds { .. }) => {}
        other => panic!("expected IndexOutOfBounds, got {other:?}"),
    }
}

#[test]
fn for_loop_fills_and_sums_array() {
    // Proves S2.4 (assignment) + S2.5 (for) + S6.2 (arrays) compose: fill
    // a[i] = i for i in 0..5, then sum the elements. 0+1+2+3+4 = 10.
    let src = "fn main() {\n    let a = [0, 0, 0, 0, 0];\n    for i in 0..5 {\n        a[i] = i;\n    }\n    let total = 0;\n    for i in 0..5 {\n        total = total + a[i];\n    }\n    total\n}\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(10));
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

// --- S6.3b: enum construction + match on variants ----------------------

#[test]
fn unit_variant_match_selects_arm_by_tag() {
    let src = "enum Light { Red, Yellow, Green }\n\
               fn main() { match Light::Green { \
                   Light::Red => 1, Light::Yellow => 2, Light::Green => 3 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(3));
}

#[test]
fn unit_variant_can_be_bound_then_matched() {
    let src = "enum Dir { N, S, E, W }\n\
               fn main() { let d = Dir::S; match d { \
                   Dir::N => 0, Dir::S => 1, Dir::E => 2, Dir::W => 3 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(1));
}

#[test]
fn tuple_variant_construct_and_bind_payload() {
    // `Box::Val(7)` builds a 1-field aggregate; the matching arm binds
    // the payload to `x`.
    let src = "enum Box { Val(Int), Empty }\n\
               fn main() { match Box::Val(7) { \
                   Box::Val(x) => x, Box::Empty => 0 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(7));
}

#[test]
fn tuple_variant_with_two_fields_binds_both() {
    let src = "enum Pair { Both(Int, Int), Neither }\n\
               fn main() { match Pair::Both(3, 4) { \
                   Pair::Both(a, b) => a + b, Pair::Neither => 0 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(7));
}

#[test]
fn tuple_variant_literal_subpattern_tests_the_field() {
    // First arm matches only when the payload equals the literal `1`.
    let matching = "enum Tag { T(Int) }\n\
                    fn main() { match Tag::T(1) { Tag::T(1) => 100, Tag::T(x) => x } }\n";
    assert_eq!(run_source(matching).unwrap(), Value::Int(100));
    // When the field differs, control falls through to the binding arm.
    let falling = "enum Tag { T(Int) }\n\
                   fn main() { match Tag::T(2) { Tag::T(1) => 100, Tag::T(x) => x } }\n";
    assert_eq!(run_source(falling).unwrap(), Value::Int(2));
}

#[test]
fn unit_variant_construction_yields_tagged_aggregate() {
    // A bare construction expression evaluates to the aggregate value.
    let src = "enum Box { Val(Int), Empty }\nfn main() { Box::Val(9) }\n";
    match run_source(src).unwrap() {
        Value::Aggregate { tag, fields } => {
            assert_eq!(tag, 0, "Val is the first declared variant");
            assert_eq!(*fields.borrow(), vec![Value::Int(9)]);
        }
        other => panic!("expected aggregate, got {other:?}"),
    }
}

#[test]
fn match_falls_through_to_wildcard_when_no_variant_matches() {
    let src = "enum Light { Red, Yellow, Green }\n\
               fn main() { match Light::Red { Light::Green => 1, _ => 99 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(99));
}

// --- S6.3c: struct construction + match on struct patterns -------------

#[test]
fn struct_literal_constructs_and_destructures() {
    let src = "struct Point { x: Int, y: Int }\n\
               fn main() { let p = Point { x: 3, y: 4 }; \
                   match p { Point { x, y } => x + y } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(7));
}

#[test]
fn struct_literal_fields_are_reordered_to_declaration_order() {
    // The literal lists `y` before `x`, but the emitter stores them in the
    // declared order [x, y], so field 0 is `x` (3) and field 1 is `y` (4).
    let src = "struct Point { x: Int, y: Int }\n\
               fn main() { let p = Point { y: 4, x: 3 }; \
                   match p { Point { x, y } => x - y } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(-1));
}

#[test]
fn struct_pattern_literal_field_tests_then_falls_through() {
    let matching = "struct P { a: Int }\n\
                    fn main() { let p = P { a: 5 }; \
                        match p { P { a: 5 } => 1, P { a } => a } }\n";
    assert_eq!(run_source(matching).unwrap(), Value::Int(1));
    let falling = "struct P { a: Int }\n\
                   fn main() { let p = P { a: 9 }; \
                       match p { P { a: 5 } => 1, P { a } => a } }\n";
    assert_eq!(run_source(falling).unwrap(), Value::Int(9));
}

#[test]
fn struct_literal_yields_tagged_aggregate() {
    let src = "struct Q { m: Int, n: Int }\nfn main() { Q { m: 1, n: 2 } }\n";
    match run_source(src).unwrap() {
        Value::Aggregate { tag, fields } => {
            assert_eq!(tag, 0, "Q is the first declared aggregate");
            assert_eq!(*fields.borrow(), vec![Value::Int(1), Value::Int(2)]);
        }
        other => panic!("expected aggregate, got {other:?}"),
    }
}

#[test]
fn struct_literal_composes_with_control_flow_blocks() {
    // Exercises the `no_struct_literal` reset inside a block body: the
    // struct literal lives in the `if` branch, not its head.
    let src = "struct Pt { x: Int, y: Int }\n\
               fn main() { if true { let p = Pt { x: 1, y: 2 }; \
                   match p { Pt { x, y } => x + y } } else { 0 } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(3));
}

#[test]
fn structs_and_enums_share_the_tag_space() {
    // A struct and an enum variant declared in the same module get
    // distinct tags from the shared counter, so matching stays correct.
    let src = "struct S { v: Int }\nenum E { A, B }\n\
               fn main() { let s = S { v: 7 }; \
                   match s { S { v } => v } }\n";
    assert_eq!(run_source(src).unwrap(), Value::Int(7));
}
