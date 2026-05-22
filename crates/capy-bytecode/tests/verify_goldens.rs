//! Verifier goldens (S5c + S7 follow-up).
//!
//! Table-driven coverage of [`verify_function`] from outside the crate:
//! one row per declared failure mode (`B0013`..=`B0020`) plus the
//! principal success shapes (straight-line, if-then-else, while, Call,
//! HostCall). Each row asserts the **stable diagnostic code** rather
//! than the exact `VerifyError` payload so the goldens stay robust
//! against future error-payload refinements while still pinning the
//! ABI contract documented in `docs/bytecode-v0.md`.
//!
//! These tests complement the inline `src/verify.rs` unit tests; the
//! inline tests verify payload shape, the goldens here pin the stable
//! `code()` surface that downstream tooling (and CapyOS at Etapa 15)
//! depends on.

use capy_bytecode::{
    verify_function, Instruction, VerifyError, B_VERIFIER_CALL_ARITY_OVERFLOW,
    B_VERIFIER_FALL_OFF_END, B_VERIFIER_INVALID_RETURN_DEPTH, B_VERIFIER_JUMP_OUT_OF_BOUNDS,
    B_VERIFIER_LOCAL_OUT_OF_BOUNDS, B_VERIFIER_STACK_INCONSISTENCY, B_VERIFIER_STACK_UNDERFLOW,
    B_VERIFIER_UNKNOWN_FUNCTION_INDEX,
};

/// Outcome a verifier golden expects.
#[derive(Debug, Clone, Copy)]
enum Outcome {
    /// Verification must succeed with the given `max_depth`.
    Ok(u32),
    /// Verification must fail with this stable diagnostic code.
    Err(&'static str),
}

/// A verifier golden row.
struct Golden {
    name: &'static str,
    instructions: Vec<Instruction>,
    locals_count: u32,
    callees: Vec<u32>,
    outcome: Outcome,
}

fn run(g: &Golden) {
    let actual = verify_function(&g.instructions, g.locals_count, &g.callees);
    match (&g.outcome, &actual) {
        (Outcome::Ok(expected), Ok(report)) => assert_eq!(
            report.max_depth, *expected,
            "{}: max_depth mismatch",
            g.name
        ),
        (Outcome::Err(expected_code), Err(err)) => assert_eq!(
            err.code(),
            *expected_code,
            "{}: code mismatch (err = {err:?})",
            g.name
        ),
        (Outcome::Ok(_), Err(err)) => {
            panic!("{}: expected Ok, got {err:?}", g.name);
        }
        (Outcome::Err(expected_code), Ok(report)) => {
            panic!(
                "{}: expected Err {expected_code}, got Ok({:?})",
                g.name, report
            );
        }
    }
}

// ---------------------------------------------------------------------
// Success shapes
// ---------------------------------------------------------------------

#[test]
fn ok_straight_line_load_add_return() {
    // load_const 0; load_const 1; add; return
    run(&Golden {
        name: "ok_straight_line",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(2),
    });
}

#[test]
fn ok_if_then_else_balanced() {
    // Lowering shape for `if c { 1 } else { 2 }`:
    //   i=0 LoadFalse       off=0   (1 byte)
    //   i=1 JumpIfFalse(10) off=1   ; after-imm=6, target=16 (i=4 else)
    //   i=2 LoadConst(0)    off=6   then: push 1
    //   i=3 Jump(5)         off=11  ; after-imm=16, target=21 (i=5 join)
    //   i=4 LoadConst(1)    off=16  else: push 1
    //   i=5 Return          off=21  join: depth 1 on both paths
    run(&Golden {
        name: "ok_if_then_else_balanced",
        instructions: vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(10),
            Instruction::LoadConst(0),
            Instruction::Jump(5),
            Instruction::LoadConst(1),
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(1),
    });
}

#[test]
fn ok_while_pattern() {
    // Lowering shape for `while c { ; }`:
    //   i=0 LoadTrue         off=0   (1 byte)
    //   i=1 JumpIfFalse(5)   off=1   ; after-imm=6,  target=11 (i=3)
    //   i=2 Jump(-11)        off=6   ; after-imm=11, target=0  (i=0 back-edge)
    //   i=3 LoadNone         off=11
    //   i=4 Return           off=16
    run(&Golden {
        name: "ok_while_pattern",
        instructions: vec![
            Instruction::LoadTrue,
            Instruction::JumpIfFalse(5),
            Instruction::Jump(-11),
            Instruction::LoadNone,
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(1),
    });
}

#[test]
fn ok_direct_call() {
    // load_const 0; load_const 1; call(0, 2); return
    run(&Golden {
        name: "ok_direct_call",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Call { fn_idx: 0, argc: 2 },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![2],
        outcome: Outcome::Ok(2),
    });
}

#[test]
fn ok_host_call_zero_args() {
    // host_call(0, 0); return
    //
    // HostCall has no static `import_idx` bounds check at verify time
    // (deferred to runtime so the existing `verify_function` signature
    // stays additive). The stack effect is the same as Call: pop argc,
    // push 1.
    run(&Golden {
        name: "ok_host_call_zero_args",
        instructions: vec![
            Instruction::HostCall {
                import_idx: 0,
                argc: 0,
            },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(1),
    });
}

#[test]
fn ok_host_call_with_arg() {
    // load_const 0; host_call(0, 1); return
    run(&Golden {
        name: "ok_host_call_with_arg",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::HostCall {
                import_idx: 0,
                argc: 1,
            },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(1),
    });
}

// ---------------------------------------------------------------------
// Failure modes (one per B0013..=B0020)
// ---------------------------------------------------------------------

#[test]
fn b0013_stack_underflow() {
    // load_const 0; add (needs 2 operands, has 1); return
    run(&Golden {
        name: "b0013_stack_underflow",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::Add,
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_STACK_UNDERFLOW),
    });
}

#[test]
fn b0014_stack_inconsistency() {
    // Then-branch pushes 1 value; else-branch pushes 2; both reach
    // the same Return so the join point sees divergent depths.
    //
    //   i=0 LoadFalse              off=0   (1 byte)
    //   i=1 JumpIfFalse(10)        off=1   ; after-imm=6, target=16 (i=4)
    //   i=2 LoadConst(0)           off=6   then: depth 1
    //   i=3 Jump(10)               off=11  ; after-imm=16, target=26 (i=6)
    //   i=4 LoadConst(1)           off=16  else: depth 1
    //   i=5 LoadConst(2)           off=21  else: depth 2
    //   i=6 Return                 off=26  join: depth 1 vs 2 → B0014
    run(&Golden {
        name: "b0014_stack_inconsistency",
        instructions: vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(10),
            Instruction::LoadConst(0),
            Instruction::Jump(10),
            Instruction::LoadConst(1),
            Instruction::LoadConst(2),
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_STACK_INCONSISTENCY),
    });
}

#[test]
fn b0015_fall_off_end() {
    // load_none (no Return after).
    run(&Golden {
        name: "b0015_fall_off_end",
        instructions: vec![Instruction::LoadNone],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_FALL_OFF_END),
    });
}

#[test]
fn b0015_empty_function_falls_off_end() {
    // An empty function has no Return; treat as fall-off at index 0.
    run(&Golden {
        name: "b0015_empty_function",
        instructions: vec![],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_FALL_OFF_END),
    });
}

#[test]
fn b0016_invalid_return_depth() {
    // Two values on the stack; Return only pops one.
    run(&Golden {
        name: "b0016_invalid_return_depth",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_INVALID_RETURN_DEPTH),
    });
}

#[test]
fn b0017_local_out_of_bounds() {
    // LoadLocal(5) with declared locals_count=2.
    run(&Golden {
        name: "b0017_local_out_of_bounds",
        instructions: vec![Instruction::LoadLocal(5), Instruction::Return],
        locals_count: 2,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_LOCAL_OUT_OF_BOUNDS),
    });
}

#[test]
fn b0017_store_local_out_of_bounds() {
    // StoreLocal(7) with declared locals_count=1.
    run(&Golden {
        name: "b0017_store_local_out_of_bounds",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::StoreLocal(7),
            Instruction::LoadNone,
            Instruction::Return,
        ],
        locals_count: 1,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_LOCAL_OUT_OF_BOUNDS),
    });
}

#[test]
fn b0018_jump_out_of_bounds() {
    // JumpIfFalse +1 lands inside the next instruction's immediate.
    // (Next valid boundary would be +5 for a 5-byte instruction.)
    run(&Golden {
        name: "b0018_jump_out_of_bounds",
        instructions: vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(1),
            Instruction::LoadConst(0),
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_JUMP_OUT_OF_BOUNDS),
    });
}

#[test]
fn b0019_unknown_function_index() {
    // Call references fn_idx=5 with a single-entry callee table.
    run(&Golden {
        name: "b0019_unknown_function_index",
        instructions: vec![
            Instruction::Call { fn_idx: 5, argc: 0 },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![0],
        outcome: Outcome::Err(B_VERIFIER_UNKNOWN_FUNCTION_INDEX),
    });
}

#[test]
fn b0020_call_arity_overflow() {
    // Callee declared `locals_count = 0` but call passes argc=1.
    run(&Golden {
        name: "b0020_call_arity_overflow",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::Call { fn_idx: 0, argc: 1 },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![0],
        outcome: Outcome::Err(B_VERIFIER_CALL_ARITY_OVERFLOW),
    });
}

// ---------------------------------------------------------------------
// HostCall + verifier interaction
// ---------------------------------------------------------------------

#[test]
fn host_call_underflow_propagates_b0013() {
    // host_call(0, 2) but the stack only has 1 value.
    run(&Golden {
        name: "host_call_underflow",
        instructions: vec![
            Instruction::LoadConst(0),
            Instruction::HostCall {
                import_idx: 0,
                argc: 2,
            },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Err(B_VERIFIER_STACK_UNDERFLOW),
    });
}

#[test]
fn host_call_does_not_check_import_idx_at_verify_time() {
    // The verifier intentionally accepts any `import_idx` value; the
    // bounds check is deferred to the VM's `V0014 UNKNOWN_HOST_IMPORT`
    // runtime trap. This documents the additive-evolution discipline
    // for the `verify_function` signature (see CHANGELOG S7 entry).
    run(&Golden {
        name: "host_call_import_idx_not_checked",
        instructions: vec![
            Instruction::HostCall {
                import_idx: 9999,
                argc: 0,
            },
            Instruction::Return,
        ],
        locals_count: 0,
        callees: vec![],
        outcome: Outcome::Ok(1),
    });
}

// ---------------------------------------------------------------------
// Code stability — stable diagnostic codes do not drift
// ---------------------------------------------------------------------

#[test]
fn stable_codes_match_documented_catalogue() {
    // `docs/compatibility.md` and `docs/bytecode-v0.md` document the
    // `B0013`..=`B0020` verifier code catalogue. This test pins the
    // mapping from the `VerifyError` payload to its `code()` so a
    // future renumbering breaks the test loudly rather than silently
    // bumping the cross-repo ABI.
    let cases: &[(VerifyError, &'static str)] = &[
        (
            VerifyError::StackUnderflow {
                ins_index: 0,
                depth: 0,
                required: 1,
            },
            "B0013",
        ),
        (
            VerifyError::StackInconsistency {
                ins_index: 0,
                prev_depth: 0,
                new_depth: 1,
            },
            "B0014",
        ),
        (
            VerifyError::FallOffEnd {
                ins_index: 0,
                depth: 0,
            },
            "B0015",
        ),
        (
            VerifyError::InvalidReturnDepth {
                ins_index: 0,
                depth: 2,
            },
            "B0016",
        ),
        (
            VerifyError::LocalOutOfBounds {
                ins_index: 0,
                slot: 5,
                locals_count: 2,
            },
            "B0017",
        ),
        (
            VerifyError::JumpOutOfBounds {
                ins_index: 0,
                target: 0,
            },
            "B0018",
        ),
        (
            VerifyError::UnknownFunctionIndex {
                ins_index: 0,
                fn_idx: 5,
                table_len: 0,
            },
            "B0019",
        ),
        (
            VerifyError::CallArityOverflow {
                ins_index: 0,
                fn_idx: 0,
                argc: 1,
                callee_locals_count: 0,
            },
            "B0020",
        ),
    ];
    for (err, expected_code) in cases {
        assert_eq!(err.code(), *expected_code, "code drift for {err:?}");
    }
}
