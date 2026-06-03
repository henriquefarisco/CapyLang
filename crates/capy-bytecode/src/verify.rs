//! Static verifier for a single function's instruction stream (v0).
//!
//! Goal: catch stack-discipline and structural defects at load time so
//! the VM only ever executes functions whose operand-stack behaviour is
//! provably consistent. The verifier walks the control-flow graph of a
//! function from entry, tracking the operand-stack depth on entry to
//! every reachable instruction and ensuring that:
//!
//! - every opcode has enough operands available (no underflow);
//! - every reachable instruction has a single, agreed depth-on-entry
//!   across all control-flow predecessors (no inconsistency);
//! - every reachable path terminates via [`Instruction::Return`] with
//!   exactly one value on the operand stack (no fall-off-end, no
//!   ambiguous return arity);
//! - every `LoadLocal` / `StoreLocal` slot is inside the declared
//!   `locals_count`;
//! - every `Jump` / `JumpIfFalse` target lands on an instruction
//!   boundary inside the same function (no jump into the middle of an
//!   immediate, no jump past the end);
//! - every `Call (fn_idx, argc)` references a known function and
//!   passes no more arguments than the callee can accept in its
//!   `locals[0..argc]` window.
//!
//! The verifier is pure: no I/O, no global state, no allocation beyond
//! what the worklist needs. It is intended to run inside the VM's
//! `from_module` path (load-time, fail-closed) and inside emitter unit
//! tests as a self-check.
//!
//! Stack-effects table (v0):
//!
//! | Opcode                                          | Requires | Delta |
//! |-------------------------------------------------|---------:|------:|
//! | `Nop`                                           |        0 |     0 |
//! | `Pop`                                           |        1 |    -1 |
//! | `LoadConst`, `LoadTrue`, `LoadFalse`, `LoadNone`|        0 |    +1 |
//! | `LoadLocal`                                     |        0 |    +1 |
//! | `StoreLocal`                                    |        1 |    -1 |
//! | `Add` `Sub` `Mul` `Div` `Mod`                   |        2 |    -1 |
//! | `Neg`                                           |        1 |     0 |
//! | `Eq` `Ne` `Lt` `Le` `Gt` `Ge`                   |        2 |    -1 |
//! | `Not`                                           |        1 |     0 |
//! | `Jump`                                          |        0 |     0 |
//! | `JumpIfFalse`                                   |        1 |    -1 |
//! | `Call(fn_idx, argc)`                            |    `argc`| `1 - argc` |
//! | `Return`                                        |        1 | terminates |

#![forbid(unsafe_code)]

use std::collections::HashMap;

use crate::instruction::Instruction;

/// Stable diagnostic codes for the static verifier. Codes are part of
/// the `capy-lang-host` v0 ABI (bytecode-side surface). The catalogue
/// is additive within v0; renaming or removing a code is breaking.
///
/// Reachable instruction would consume more operands than the current
/// stack provides.
pub const B_VERIFIER_STACK_UNDERFLOW: &str = "B0013";
/// Two control-flow predecessors of the same instruction disagree on
/// the operand-stack depth.
pub const B_VERIFIER_STACK_INCONSISTENCY: &str = "B0014";
/// A reachable path ran out of instructions without executing a
/// terminating `Return`.
pub const B_VERIFIER_FALL_OFF_END: &str = "B0015";
/// A `Return` was reached with an operand-stack depth that is not
/// exactly one (no return value, or unbalanced stack residue).
pub const B_VERIFIER_INVALID_RETURN_DEPTH: &str = "B0016";
/// `LoadLocal` / `StoreLocal` referenced a slot beyond the function's
/// declared `locals_count`.
pub const B_VERIFIER_LOCAL_OUT_OF_BOUNDS: &str = "B0017";
/// `Jump` / `JumpIfFalse` target lands outside the current function or
/// in the middle of an instruction's immediate.
pub const B_VERIFIER_JUMP_OUT_OF_BOUNDS: &str = "B0018";
/// `Call` referenced a function index that is out of bounds for the
/// module's function table.
pub const B_VERIFIER_UNKNOWN_FUNCTION_INDEX: &str = "B0019";
/// `Call`'s `argc` exceeds the callee's declared `locals_count`.
pub const B_VERIFIER_CALL_ARITY_OVERFLOW: &str = "B0020";

/// One reason a function failed verification. Carries the failing
/// instruction index plus a stable diagnostic code via
/// [`VerifyError::code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    StackUnderflow {
        ins_index: usize,
        depth: u32,
        required: u32,
    },
    StackInconsistency {
        ins_index: usize,
        prev_depth: u32,
        new_depth: u32,
    },
    FallOffEnd {
        ins_index: usize,
        depth: u32,
    },
    InvalidReturnDepth {
        ins_index: usize,
        depth: u32,
    },
    LocalOutOfBounds {
        ins_index: usize,
        slot: u32,
        locals_count: u32,
    },
    JumpOutOfBounds {
        ins_index: usize,
        target: i64,
    },
    UnknownFunctionIndex {
        ins_index: usize,
        fn_idx: u32,
        table_len: u32,
    },
    CallArityOverflow {
        ins_index: usize,
        fn_idx: u32,
        argc: u32,
        callee_locals_count: u32,
    },
}

impl VerifyError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StackUnderflow { .. } => B_VERIFIER_STACK_UNDERFLOW,
            Self::StackInconsistency { .. } => B_VERIFIER_STACK_INCONSISTENCY,
            Self::FallOffEnd { .. } => B_VERIFIER_FALL_OFF_END,
            Self::InvalidReturnDepth { .. } => B_VERIFIER_INVALID_RETURN_DEPTH,
            Self::LocalOutOfBounds { .. } => B_VERIFIER_LOCAL_OUT_OF_BOUNDS,
            Self::JumpOutOfBounds { .. } => B_VERIFIER_JUMP_OUT_OF_BOUNDS,
            Self::UnknownFunctionIndex { .. } => B_VERIFIER_UNKNOWN_FUNCTION_INDEX,
            Self::CallArityOverflow { .. } => B_VERIFIER_CALL_ARITY_OVERFLOW,
        }
    }
}

/// Successful verification report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Maximum operand-stack depth observed across all reachable
    /// instructions. Useful for sizing pre-allocated VM stacks in a
    /// future budget-aware pass.
    pub max_depth: u32,
}

/// Verifies one function's instruction stream against the v0 stack
/// discipline. `callee_locals_counts[i]` must be the `locals_count`
/// declared by function index `i` in the module so cross-function
/// `Call` validation can be performed without a module reference.
pub fn verify_function(
    instructions: &[Instruction],
    locals_count: u32,
    callee_locals_counts: &[u32],
) -> Result<VerifyReport, VerifyError> {
    if instructions.is_empty() {
        // An empty function body has no terminating `Return`; treat
        // this exactly like falling off the end at index 0.
        return Err(VerifyError::FallOffEnd {
            ins_index: 0,
            depth: 0,
        });
    }

    // Precompute byte offsets so `Jump` / `JumpIfFalse` targets can be
    // resolved back to an instruction index in O(1). The mapping uses
    // the byte that follows the full instruction (PC after decoding
    // the immediate) as the relative base, matching the encoding
    // contract documented in `docs/bytecode-v0.md`.
    let mut byte_offsets: Vec<u32> = Vec::with_capacity(instructions.len());
    let mut off: u32 = 0;
    for ins in instructions {
        byte_offsets.push(off);
        // `width()` is at most 9 (opcode + 8-byte immediate) so the
        // checked_add is purely defensive against a future immediate
        // shape; failure here cannot happen in v0 inputs.
        off = match off.checked_add(ins.width() as u32) {
            Some(v) => v,
            None => {
                return Err(VerifyError::JumpOutOfBounds {
                    ins_index: byte_offsets.len() - 1,
                    target: i64::MAX,
                });
            }
        };
    }
    let mut offset_to_index: HashMap<u32, usize> = HashMap::with_capacity(instructions.len());
    for (i, &b) in byte_offsets.iter().enumerate() {
        offset_to_index.insert(b, i);
    }

    let mut visited: HashMap<usize, u32> = HashMap::with_capacity(instructions.len());
    let mut worklist: Vec<(usize, u32)> = Vec::new();
    worklist.push((0, 0));
    let mut max_depth: u32 = 0;

    while let Some((i, depth)) = worklist.pop() {
        if i >= instructions.len() {
            return Err(VerifyError::FallOffEnd {
                ins_index: i,
                depth,
            });
        }
        if let Some(&prev) = visited.get(&i) {
            if prev != depth {
                return Err(VerifyError::StackInconsistency {
                    ins_index: i,
                    prev_depth: prev,
                    new_depth: depth,
                });
            }
            continue;
        }
        visited.insert(i, depth);
        if depth > max_depth {
            max_depth = depth;
        }

        let ins = instructions[i];
        let required = required_inputs(ins);
        if depth < required {
            return Err(VerifyError::StackUnderflow {
                ins_index: i,
                depth,
                required,
            });
        }

        // Index validations that do not depend on the resulting depth.
        match ins {
            Instruction::LoadLocal(slot) | Instruction::StoreLocal(slot)
                if slot >= locals_count =>
            {
                return Err(VerifyError::LocalOutOfBounds {
                    ins_index: i,
                    slot,
                    locals_count,
                });
            }
            Instruction::Call { fn_idx, argc } => {
                let table_len = callee_locals_counts.len() as u32;
                if fn_idx >= table_len {
                    return Err(VerifyError::UnknownFunctionIndex {
                        ins_index: i,
                        fn_idx,
                        table_len,
                    });
                }
                let callee_locals_count = callee_locals_counts[fn_idx as usize];
                if argc > callee_locals_count {
                    return Err(VerifyError::CallArityOverflow {
                        ins_index: i,
                        fn_idx,
                        argc,
                        callee_locals_count,
                    });
                }
            }
            _ => {}
        }

        // Compute depth after `ins` executes and dispatch successors.
        let new_depth = depth - required + produced_outputs(ins);

        match ins {
            Instruction::Return => {
                // `Return` pops exactly one value. We already checked
                // `depth >= 1` via `required_inputs`; now ensure no
                // residue remains underneath.
                if depth != 1 {
                    return Err(VerifyError::InvalidReturnDepth {
                        ins_index: i,
                        depth,
                    });
                }
                // No successor: the function terminates here.
            }
            Instruction::Jump(offset) => {
                let after = byte_offsets[i] as i64 + ins.width() as i64;
                let target = after + offset as i64;
                let idx = resolve_jump_target(&offset_to_index, target, i)?;
                worklist.push((idx, new_depth));
            }
            Instruction::JumpIfFalse(offset) => {
                let after = byte_offsets[i] as i64 + ins.width() as i64;
                let target = after + offset as i64;
                let idx = resolve_jump_target(&offset_to_index, target, i)?;
                // Both arms reach with the post-pop depth.
                worklist.push((idx, new_depth));
                let next = i + 1;
                if next >= instructions.len() {
                    return Err(VerifyError::FallOffEnd {
                        ins_index: next,
                        depth: new_depth,
                    });
                }
                worklist.push((next, new_depth));
            }
            _ => {
                let next = i + 1;
                if next >= instructions.len() {
                    return Err(VerifyError::FallOffEnd {
                        ins_index: next,
                        depth: new_depth,
                    });
                }
                worklist.push((next, new_depth));
            }
        }
    }

    Ok(VerifyReport { max_depth })
}

fn required_inputs(ins: Instruction) -> u32 {
    match ins {
        Instruction::Nop
        | Instruction::LoadConst(_)
        | Instruction::LoadTrue
        | Instruction::LoadFalse
        | Instruction::LoadNone
        | Instruction::LoadLocal(_)
        | Instruction::Jump(_) => 0,
        Instruction::Pop
        | Instruction::StoreLocal(_)
        | Instruction::Neg
        | Instruction::Not
        | Instruction::BitNot
        | Instruction::ArrayLen
        | Instruction::GetField(_)
        | Instruction::GetTag
        | Instruction::JumpIfFalse(_)
        | Instruction::Return => 1,
        Instruction::Add
        | Instruction::Sub
        | Instruction::Mul
        | Instruction::Div
        | Instruction::Mod
        | Instruction::BitAnd
        | Instruction::BitOr
        | Instruction::BitXor
        | Instruction::Shl
        | Instruction::Shr
        | Instruction::Eq
        | Instruction::Ne
        | Instruction::Lt
        | Instruction::Le
        | Instruction::Gt
        | Instruction::Ge
        | Instruction::ArrayGet => 2,
        Instruction::ArraySet => 3,
        Instruction::MakeArray(n) => n,
        Instruction::MakeAggregate { field_count, .. } => field_count,
        Instruction::Call { argc, .. } | Instruction::HostCall { argc, .. } => argc,
    }
}

fn produced_outputs(ins: Instruction) -> u32 {
    match ins {
        Instruction::Nop
        | Instruction::Pop
        | Instruction::StoreLocal(_)
        | Instruction::Jump(_)
        | Instruction::JumpIfFalse(_)
        | Instruction::Return => 0,
        Instruction::LoadConst(_)
        | Instruction::LoadTrue
        | Instruction::LoadFalse
        | Instruction::LoadNone
        | Instruction::LoadLocal(_)
        | Instruction::Neg
        | Instruction::Not
        | Instruction::BitNot
        | Instruction::Add
        | Instruction::Sub
        | Instruction::Mul
        | Instruction::Div
        | Instruction::Mod
        | Instruction::BitAnd
        | Instruction::BitOr
        | Instruction::BitXor
        | Instruction::Shl
        | Instruction::Shr
        | Instruction::Eq
        | Instruction::Ne
        | Instruction::Lt
        | Instruction::Le
        | Instruction::Gt
        | Instruction::Ge
        | Instruction::MakeArray(_)
        | Instruction::ArrayGet
        | Instruction::ArraySet
        | Instruction::ArrayLen
        | Instruction::MakeAggregate { .. }
        | Instruction::GetField(_)
        | Instruction::GetTag
        | Instruction::Call { .. }
        | Instruction::HostCall { .. } => 1,
    }
}

fn resolve_jump_target(
    offset_to_index: &HashMap<u32, usize>,
    target: i64,
    ins_index: usize,
) -> Result<usize, VerifyError> {
    if target < 0 || target > u32::MAX as i64 {
        return Err(VerifyError::JumpOutOfBounds { ins_index, target });
    }
    let t = target as u32;
    offset_to_index
        .get(&t)
        .copied()
        .ok_or(VerifyError::JumpOutOfBounds { ins_index, target })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify(
        instructions: &[Instruction],
        locals: u32,
        callees: &[u32],
    ) -> Result<u32, VerifyError> {
        verify_function(instructions, locals, callees).map(|r| r.max_depth)
    }

    #[test]
    fn straight_line_add_then_return_passes() {
        // load_const 0; load_const 1; add; return
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ];
        assert_eq!(verify(&ins, 0, &[]).unwrap(), 2);
    }

    #[test]
    fn empty_function_is_rejected() {
        let err = verify(&[], 0, &[]).unwrap_err();
        assert!(matches!(err, VerifyError::FallOffEnd { .. }));
    }

    #[test]
    fn missing_return_falls_off_end() {
        let ins = vec![Instruction::LoadNone];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::FallOffEnd { .. } => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn return_with_extra_residue_traps() {
        // Two values on the stack; Return only pops one.
        let ins = vec![
            Instruction::LoadNone,
            Instruction::LoadNone,
            Instruction::Return,
        ];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::InvalidReturnDepth { depth, .. } => assert_eq!(depth, 2),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn underflow_on_add_with_one_operand_traps() {
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::Add,
            Instruction::Return,
        ];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::StackUnderflow {
                required, depth, ..
            } => {
                assert_eq!(required, 2);
                assert_eq!(depth, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn local_out_of_bounds_traps() {
        let ins = vec![Instruction::LoadLocal(5), Instruction::Return];
        match verify(&ins, 2, &[]).unwrap_err() {
            VerifyError::LocalOutOfBounds {
                slot, locals_count, ..
            } => {
                assert_eq!(slot, 5);
                assert_eq!(locals_count, 2);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn if_then_else_balanced_passes() {
        // Lowering shape for `if c { 1 } else { 2 }`:
        //   0000  load_false       ; cond (1 byte)
        //   0001  jump_if_false +5 ; -> 000b (else)         (5 bytes)
        //   0006  load_const 1     ; then                   (5 bytes)
        //   000b  jump +1          ; -> 0010 (end)          (5 bytes)
        // Wait — Jump width is 5 (opcode + I32). After-imm pc for the
        // Jump at byte 0006 is 000b; jumping +5 lands at 0010 = end_lbl.
        //   0010  load_const 2     ; else
        //   0015  return
        // Actually for the verifier the *exact* offsets don't matter:
        // we encode the instructions directly. The Jump offset is the
        // byte-distance to the target measured from after the
        // immediate. Let's lay out the instructions and compute.
        //
        // byte_offsets per instruction (widths: load_false=1,
        // jump_if_false=5, load_const=5, jump=5, load_const=5, return=1):
        //   i=0 LoadFalse              off=0
        //   i=1 JumpIfFalse(o)         off=1   ; after-imm = 6
        //   i=2 LoadConst(0)           off=6
        //   i=3 Jump(o2)               off=11  ; after-imm = 16
        //   i=4 LoadConst(1)           off=16
        //   i=5 Return                 off=21
        //
        // Want JumpIfFalse to target the else (i=4 @ off=16):
        //   offset = 16 - 6 = 10.
        // Want Jump to skip the else (i=5 @ off=21):
        //   offset = 21 - 16 = 5.
        let ins = vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(10),
            Instruction::LoadConst(0),
            Instruction::Jump(5),
            Instruction::LoadConst(1),
            Instruction::Return,
        ];
        // Both arms push one value; depth at Return = 1.
        let max = verify(&ins, 0, &[]).unwrap();
        assert_eq!(max, 1);
    }

    #[test]
    fn divergent_depths_at_join_point_trap() {
        // Then branch pushes 1 value; else branch pushes 2. At the
        // join (Return), the two paths disagree.
        //
        //   i=0 LoadFalse              off=0   (1 byte)
        //   i=1 JumpIfFalse(o)         off=1   ; after-imm = 6  (5 bytes)
        //   i=2 LoadConst(0)           off=6   (5 bytes)
        //   i=3 Jump(o2)               off=11  ; after-imm = 16 (5 bytes)
        //   i=4 LoadConst(1)           off=16  (5 bytes)
        //   i=5 LoadConst(2)           off=21  (5 bytes)
        //   i=6 Return                 off=26  (1 byte)
        //
        // JIF target = i=4 (off=16):  16 - 6 = 10.
        // Jump target = i=6 (off=26): 26 - 16 = 10.
        let ins = vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(10),
            Instruction::LoadConst(0),
            Instruction::Jump(10),
            Instruction::LoadConst(1),
            Instruction::LoadConst(2),
            Instruction::Return,
        ];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::StackInconsistency { .. } => {}
            VerifyError::InvalidReturnDepth { .. } => {
                // Either inconsistency at Return or invalid-return-
                // depth is an acceptable rejection here, depending
                // on which arm the worklist explores first.
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn jump_to_middle_of_immediate_traps() {
        // JumpIfFalse +1 would land 1 byte past after-imm. The next
        // valid boundary is +5 (start of next 5-byte instruction).
        //
        //   i=0 LoadFalse        off=0  (1 byte)
        //   i=1 JumpIfFalse(o)   off=1  ; after-imm = 6 (5 bytes)
        //   i=2 LoadConst(0)     off=6
        //   i=3 Return           off=11
        // o=1 → target = 6 + 1 = 7, which is inside LoadConst's
        // immediate (no instruction starts at byte 7).
        let ins = vec![
            Instruction::LoadFalse,
            Instruction::JumpIfFalse(1),
            Instruction::LoadConst(0),
            Instruction::Return,
        ];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::JumpOutOfBounds { target, .. } => assert_eq!(target, 7),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn while_loop_pattern_passes() {
        // Equivalent to `while c { ; } -> ()`:
        //
        //   i=0 LoadTrue           off=0   (1)
        //   i=1 JumpIfFalse(o1)    off=1   ; after-imm=6, target=i=3 (off=11) → o1=5
        //   i=2 Jump(o2)           off=6   ; after-imm=11, target=i=0 (off=0) → o2=-11
        //   i=3 LoadNone           off=11
        //   i=4 Return             off=16
        let ins = vec![
            Instruction::LoadTrue,
            Instruction::JumpIfFalse(5),
            Instruction::Jump(-11),
            Instruction::LoadNone,
            Instruction::Return,
        ];
        let max = verify(&ins, 0, &[]).unwrap();
        assert_eq!(max, 1);
    }

    #[test]
    fn call_with_valid_arity_passes() {
        // load_const 0; load_const 1; call(0, 2); return
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Call { fn_idx: 0, argc: 2 },
            Instruction::Return,
        ];
        // Callee 0 has locals_count >= 2.
        let max = verify(&ins, 0, &[2]).unwrap();
        assert_eq!(max, 2);
    }

    #[test]
    fn call_with_unknown_fn_idx_traps() {
        let ins = vec![
            Instruction::Call { fn_idx: 5, argc: 0 },
            Instruction::Return,
        ];
        match verify(&ins, 0, &[0]).unwrap_err() {
            VerifyError::UnknownFunctionIndex {
                fn_idx, table_len, ..
            } => {
                assert_eq!(fn_idx, 5);
                assert_eq!(table_len, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn call_with_arity_overflow_traps() {
        // Callee has locals_count = 0, but argc = 1.
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::Call { fn_idx: 0, argc: 1 },
            Instruction::Return,
        ];
        match verify(&ins, 0, &[0]).unwrap_err() {
            VerifyError::CallArityOverflow {
                argc,
                callee_locals_count,
                ..
            } => {
                assert_eq!(argc, 1);
                assert_eq!(callee_locals_count, 0);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn aggregate_build_and_get_field_passes() {
        // load_const 0; load_const 1; make_aggregate(tag=0, 2);
        // get_field 0; return — net stack effect leaves one value.
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::MakeAggregate {
                tag: 0,
                field_count: 2,
            },
            Instruction::GetField(0),
            Instruction::Return,
        ];
        // Peak depth is 2 (both fields pushed before make_aggregate).
        assert_eq!(verify(&ins, 0, &[]).unwrap(), 2);
    }

    #[test]
    fn aggregate_underflow_when_fields_missing_traps() {
        // make_aggregate wants 2 operands but the stack only has 1.
        let ins = vec![
            Instruction::LoadConst(0),
            Instruction::MakeAggregate {
                tag: 0,
                field_count: 2,
            },
            Instruction::Return,
        ];
        match verify(&ins, 0, &[]).unwrap_err() {
            VerifyError::StackUnderflow {
                required, depth, ..
            } => {
                assert_eq!(required, 2);
                assert_eq!(depth, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
