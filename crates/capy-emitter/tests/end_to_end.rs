//! End-to-end emitter tests: source → AST → bytecode `Module` → decoded
//! instruction stream.

use capy_bytecode::{
    decode, ConstPool, Constant, FunctionTable, ImportTable, Instruction, Module, SectionTag,
};
use capy_emitter::emit;
use capy_parser::parse_source;

fn decode_first_fn(module: &Module) -> (FunctionTable, ConstPool, Vec<Instruction>) {
    let consts_section = module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Consts)
        .expect("consts section");
    let consts = ConstPool::decode(&consts_section.payload).expect("decode consts");
    let functions_section = module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    let functions = FunctionTable::decode(&functions_section.payload).expect("decode functions");
    assert!(!functions.entries.is_empty(), "expected at least one fn");
    let instructions = decode(&functions.entries[0].code).expect("decode instructions");
    (functions, consts, instructions)
}

#[test]
fn empty_fn_lowers_to_load_none_return() {
    let parsed = parse_source("fn nop() {}\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert!(consts.entries.is_empty());
    assert_eq!(ins, vec![Instruction::LoadNone, Instruction::Return]);
}

#[test]
fn simple_arithmetic_tail_returns_value() {
    let parsed = parse_source("fn add() { 1 + 2 }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(1), Constant::Int(2)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ]
    );
}

#[test]
fn let_then_use_uses_locals_machinery() {
    let parsed = parse_source("fn use_let() { let x = 42; x }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(42)]);
    assert_eq!(table.entries[0].locals_count, 1);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::StoreLocal(0),
            Instruction::LoadLocal(0),
            Instruction::Return,
        ]
    );
}

#[test]
fn if_else_emits_branching_pattern() {
    let parsed = parse_source("fn cond() { if 1 == 2 { 10 } else { 20 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(
        consts.entries,
        vec![
            Constant::Int(1),
            Constant::Int(2),
            Constant::Int(10),
            Constant::Int(20),
        ]
    );

    // Walk the structure rather than hard-coding raw offsets so the
    // assertion stays robust if we extend the lowering. Layout:
    //   load_const 0     ; 1
    //   load_const 1     ; 2
    //   eq
    //   jump_if_false <else>
    //   load_const 2     ; 10  (then)
    //   jump <end>
    //   load_const 3     ; 20  (else)
    //   return
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::LoadConst(1));
    assert_eq!(ins[2], Instruction::Eq);
    assert!(matches!(ins[3], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[4], Instruction::LoadConst(2));
    assert!(matches!(ins[5], Instruction::Jump(_)));
    assert_eq!(ins[6], Instruction::LoadConst(3));
    assert_eq!(ins[7], Instruction::Return);
    assert_eq!(ins.len(), 8);
}

#[test]
fn explicit_return_emits_return_then_dead_wrapper() {
    let parsed = parse_source("fn early() { return 7; }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // The body contains a `return 7;` statement followed by an implicit
    // tail (LoadNone) and the wrapper Return. The Pop is the always-Pop
    // applied to every expression statement; the trailing instructions
    // are unreachable but harmless and deterministic.
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::Return);
    // Dead-code tail:
    assert_eq!(ins[2], Instruction::Pop);
    assert_eq!(ins[3], Instruction::LoadNone);
    assert_eq!(ins[4], Instruction::Return);
}

#[test]
fn const_pool_dedup_works_across_a_function() {
    let parsed = parse_source("fn dedup() { 1 + 1 + 1 }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    // `1` is interned exactly once.
    assert_eq!(consts.entries, vec![Constant::Int(1)]);
    // `(1 + 1) + 1` ⇒ LoadConst 0; LoadConst 0; Add; LoadConst 0; Add; Return.
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(0),
            Instruction::Add,
            Instruction::LoadConst(0),
            Instruction::Add,
            Instruction::Return,
        ]
    );
}

#[test]
fn imports_pass_through() {
    // Since S7 the `as` alias renames the source-level callable only;
    // the wire `(module, symbol)` pair the host adapter binds against
    // continues to reflect the underlying import path so the surface
    // CapyOS consumes stays decoupled from local renaming.
    let parsed = parse_source("import time::now;\nimport log::info as log_info;\nfn nop() {}\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let imports_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Imports)
        .expect("imports section");
    let imports = ImportTable::decode(&imports_section.payload).expect("decode imports");
    assert_eq!(imports.entries.len(), 2);
    assert_eq!(imports.entries[0].module, "time");
    assert_eq!(imports.entries[0].symbol, "now");
    assert_eq!(imports.entries[1].module, "log");
    assert_eq!(imports.entries[1].symbol, "info");
}

#[test]
fn fn_with_params_lowers_to_locals_zero_through_n() {
    // S5b.3: parameters are registered as the first locals of the
    // function (locals[0] = x, locals[1] = y) and the body uses them
    // directly via LoadLocal.
    let parsed = parse_source("fn add(x: i32, y: i32) -> i32 { x + y }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(table.entries[0].locals_count, 2);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadLocal(0),
            Instruction::LoadLocal(1),
            Instruction::Add,
            Instruction::Return,
        ]
    );
}

// --- S5b.2: loops, break/continue, short-circuit boolean operators ---

#[test]
fn while_emits_loop_pattern_with_break_join() {
    let parsed = parse_source("fn w() { while true { 1; } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Expected lowering. The outer fn body is `{ while true { 1; } }`,
    // where the `while` expression is the block's tail value, so no
    // additional `LoadNone` follows it.
    //
    //   load_true                       ; cond
    //   jump_if_false fallthrough
    //   load_const 0                    ; body stmt: 1
    //   pop                             ; stmt discards value
    //   load_none                       ; block tail (no tail expr)
    //   pop                             ; body value discarded by while
    //   jump loop_start
    //   pop                             ; break_label
    //   load_none                       ; fallthrough: while result (= outer tail)
    //   return
    assert_eq!(ins[0], Instruction::LoadTrue);
    assert!(matches!(ins[1], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[2], Instruction::LoadConst(0));
    assert_eq!(ins[3], Instruction::Pop);
    assert_eq!(ins[4], Instruction::LoadNone);
    assert_eq!(ins[5], Instruction::Pop);
    assert!(matches!(ins[6], Instruction::Jump(_)));
    assert_eq!(ins[7], Instruction::Pop);
    assert_eq!(ins[8], Instruction::LoadNone);
    assert_eq!(ins[9], Instruction::Return);
    assert_eq!(ins.len(), 10);
}

#[test]
fn loop_emits_unconditional_back_edge() {
    let parsed = parse_source("fn l() { loop { 1; } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Layout:
    //   load_const 0   ; 1 (stmt)
    //   pop            ; stmt discard
    //   load_none      ; body block tail
    //   pop            ; body value discarded
    //   jump loop_start
    //   return         ; break_label falls through here (dead code in
    //                  ; the absence of break, but kept deterministic)
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::Pop);
    assert_eq!(ins[2], Instruction::LoadNone);
    assert_eq!(ins[3], Instruction::Pop);
    assert!(matches!(ins[4], Instruction::Jump(_)));
    assert_eq!(ins[5], Instruction::Return);
    assert_eq!(ins.len(), 6);
}

#[test]
fn break_outside_loop_is_reported() {
    let parsed = parse_source("fn b() { break; }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::BreakOutsideLoop
    ));
}

#[test]
fn continue_outside_loop_is_reported() {
    let parsed = parse_source("fn c() { continue; }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::ContinueOutsideLoop
    ));
}

#[test]
fn array_push_method_lowers_to_array_push_op() {
    // S10: `a.push(7)` lowers to receiver + arg + ArrayPush. The receiver
    // (local 0) is emitted first, then the argument, matching the opcode.
    let parsed = parse_source("fn f(a: i32) { a.push(7) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(7)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadLocal(0),
            Instruction::LoadConst(0),
            Instruction::ArrayPush,
            Instruction::Return,
        ]
    );
}

#[test]
fn array_len_method_lowers_to_array_len_op() {
    // A zero-argument method: only the receiver is emitted, then ArrayLen.
    let parsed = parse_source("fn f(a: i32) { a.len() }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    assert!(consts.entries.is_empty());
    assert_eq!(
        ins,
        vec![
            Instruction::LoadLocal(0),
            Instruction::ArrayLen,
            Instruction::Return,
        ]
    );
}

#[test]
fn array_insert_method_lowers_receiver_then_args_in_order() {
    // A two-argument method: receiver, then idx, then val (source order),
    // matching ArrayInsert's `arr idx val` operand order.
    let parsed = parse_source("fn f(a: i32) { a.insert(1, 2) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(1), Constant::Int(2)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadLocal(0),
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::ArrayInsert,
            Instruction::Return,
        ]
    );
}

#[test]
fn array_method_wrong_arity_is_reported() {
    // `push` wants exactly one argument; zero is a fail-closed E0022.
    let parsed = parse_source("fn f(a: i32) { a.push() }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert_eq!(out.errors[0].code(), "E0022");
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::MethodArity {
            want: 1,
            got: 0,
            ..
        }
    ));
}

#[test]
fn unknown_method_is_reported() {
    // Only the built-in array methods lower; anything else is fail-closed.
    let parsed = parse_source("fn f(a: i32) { a.frobnicate() }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::UnsupportedFeature { .. }
    ));
}

#[test]
fn min_builtin_lowers_to_compare_select() {
    // min(a, b) stores both args, compares with Le, and branches — the same
    // shape as an `if`. With params a=local0, b=local1 the temporaries are
    // locals 2 and 3; on `lhs <= rhs` it selects the lhs (the smaller).
    let parsed = parse_source("fn f(a: i32, b: i32) { min(a, b) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(ins[0], Instruction::LoadLocal(0)); // a
    assert_eq!(ins[1], Instruction::StoreLocal(2));
    assert_eq!(ins[2], Instruction::LoadLocal(1)); // b
    assert_eq!(ins[3], Instruction::StoreLocal(3));
    assert_eq!(ins[4], Instruction::LoadLocal(2));
    assert_eq!(ins[5], Instruction::LoadLocal(3));
    assert_eq!(ins[6], Instruction::Le);
    assert!(matches!(ins[7], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[8], Instruction::LoadLocal(2)); // lhs <= rhs -> min = a
    assert!(matches!(ins[9], Instruction::Jump(_)));
    assert_eq!(ins[10], Instruction::LoadLocal(3)); // else -> b
    assert_eq!(ins[11], Instruction::Return);
    assert_eq!(ins.len(), 12);
}

#[test]
fn max_builtin_selects_the_other_branch() {
    // max mirrors min but selects the rhs on `lhs <= rhs`.
    let parsed = parse_source("fn f(a: i32, b: i32) { max(a, b) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(ins[6], Instruction::Le);
    assert!(matches!(ins[7], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[8], Instruction::LoadLocal(3)); // lhs <= rhs -> max = b
    assert!(matches!(ins[9], Instruction::Jump(_)));
    assert_eq!(ins[10], Instruction::LoadLocal(2)); // else -> a
    assert_eq!(ins[11], Instruction::Return);
    assert_eq!(ins.len(), 12);
}

#[test]
fn min_builtin_wrong_arity_is_reported() {
    // the numeric built-ins want exactly two arguments; one is E0022.
    let parsed = parse_source("fn f(a: i32) { min(a) }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert_eq!(out.errors[0].code(), "E0022");
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::MethodArity {
            want: 2,
            got: 1,
            ..
        }
    ));
}

#[test]
fn user_fn_min_takes_precedence_over_builtin() {
    // A user-declared `fn min` resolves as an ordinary call (here one arg),
    // shadowing the two-argument numeric built-in — so no compare/select.
    let parsed = parse_source("fn min(x: i32) -> i32 { x }\nfn g() { min(7) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let functions_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    let functions = FunctionTable::decode(&functions_section.payload).expect("decode");
    let g = decode(&functions.entries[1].code).expect("decode g");
    assert!(g
        .iter()
        .any(|i| matches!(i, Instruction::Call { argc: 1, .. })));
    assert!(!g.iter().any(|i| matches!(i, Instruction::Le)));
}

#[test]
fn clamp_builtin_lowers_to_two_compare_selects() {
    // clamp(x, lo, hi) = max(lo, min(x, hi)): two compare-and-branch selects,
    // no const-pool. Params x=local0, lo=local1, hi=local2; the temporaries
    // are locals 3..=6 (the three arg copies + the inner min result).
    let parsed = parse_source("fn f(x: i32, lo: i32, hi: i32) { clamp(x, lo, hi) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    // The lowering only compares the arguments to each other: no literals.
    assert!(consts.entries.is_empty(), "consts: {:?}", consts.entries);
    // Each of the three arguments is stored into a temporary first.
    assert_eq!(ins[0], Instruction::LoadLocal(0));
    assert_eq!(ins[1], Instruction::StoreLocal(3));
    assert_eq!(ins[2], Instruction::LoadLocal(1));
    assert_eq!(ins[3], Instruction::StoreLocal(4));
    assert_eq!(ins[4], Instruction::LoadLocal(2));
    assert_eq!(ins[5], Instruction::StoreLocal(5));
    // Two compare-selects (min then max): exactly two Le and two
    // JumpIfFalse/Jump pairs, with the inner result stored once.
    assert_eq!(
        ins.iter().filter(|i| matches!(i, Instruction::Le)).count(),
        2
    );
    assert_eq!(
        ins.iter()
            .filter(|i| matches!(i, Instruction::JumpIfFalse(_)))
            .count(),
        2
    );
    assert_eq!(
        ins.iter()
            .filter(|i| matches!(i, Instruction::Jump(_)))
            .count(),
        2
    );
    assert!(ins.iter().any(|i| matches!(i, Instruction::StoreLocal(6))));
}

#[test]
fn clamp_builtin_wrong_arity_is_reported() {
    // clamp wants exactly three arguments; two is E0022.
    let parsed = parse_source("fn f(a: i32, b: i32) { clamp(a, b) }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert_eq!(out.errors[0].code(), "E0022");
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::MethodArity {
            want: 3,
            got: 2,
            ..
        }
    ));
}

#[test]
fn user_fn_clamp_takes_precedence_over_builtin() {
    // A user-declared `fn clamp` resolves as an ordinary call (here one arg),
    // shadowing the three-argument numeric built-in, so no compare/select.
    let parsed = parse_source("fn clamp(x: i32) -> i32 { x }\nfn g() { clamp(9) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let functions_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    let functions = FunctionTable::decode(&functions_section.payload).expect("decode");
    let g = decode(&functions.entries[1].code).expect("decode g");
    assert!(g
        .iter()
        .any(|i| matches!(i, Instruction::Call { argc: 1, .. })));
    assert!(!g.iter().any(|i| matches!(i, Instruction::Le)));
}

#[test]
fn abs_builtin_lowers_to_compare_neg_and_select() {
    // abs(x) = if x < 0 { -x } else { x }: compare against the literal 0 and
    // negate on the true branch. One Int(0) constant, exactly one Lt and Neg.
    let parsed = parse_source("fn f(x: i32) { abs(x) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    assert!(consts.entries.contains(&Constant::Int(0)));
    assert_eq!(
        ins.iter().filter(|i| matches!(i, Instruction::Lt)).count(),
        1
    );
    assert_eq!(
        ins.iter().filter(|i| matches!(i, Instruction::Neg)).count(),
        1
    );
}

#[test]
fn sign_builtin_lowers_to_three_way_select() {
    // sign(x): -1 if x<0, 1 if x>0, else 0 -> Int(-1)/Int(0)/Int(1), a Lt+Gt.
    let parsed = parse_source("fn f(x: i32) { sign(x) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_t, consts, ins) = decode_first_fn(&out.module);
    assert!(consts.entries.contains(&Constant::Int(-1)));
    assert!(consts.entries.contains(&Constant::Int(0)));
    assert!(consts.entries.contains(&Constant::Int(1)));
    assert_eq!(
        ins.iter().filter(|i| matches!(i, Instruction::Lt)).count(),
        1
    );
    assert_eq!(
        ins.iter().filter(|i| matches!(i, Instruction::Gt)).count(),
        1
    );
}

#[test]
fn unary_numeric_builtin_wrong_arity_is_reported() {
    // abs/sign want exactly one argument; two is E0022.
    let parsed = parse_source("fn f(a: i32, b: i32) { abs(a, b) }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert_eq!(out.errors[0].code(), "E0022");
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::MethodArity {
            want: 1,
            got: 2,
            ..
        }
    ));
}

#[test]
fn user_fn_abs_takes_precedence_over_builtin() {
    // A user-declared `fn abs` resolves as an ordinary call, shadowing the
    // numeric built-in, so no compare/negate select is emitted.
    let parsed = parse_source("fn abs(x: i32) -> i32 { x }\nfn g() { abs(9) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let functions_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    let functions = FunctionTable::decode(&functions_section.payload).expect("decode");
    let g = decode(&functions.entries[1].code).expect("decode g");
    assert!(g
        .iter()
        .any(|i| matches!(i, Instruction::Call { argc: 1, .. })));
    assert!(!g.iter().any(|i| matches!(i, Instruction::Neg)));
}

#[test]
fn break_with_value_inside_loop_compiles() {
    let parsed = parse_source("fn br() { loop { break 7; } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(7)]);
    // Body `{ break 7; }`: Stmt::Expr(Break(7)) then block tail LoadNone.
    // emit_break pushes the value then Jumps to break_label.
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert!(matches!(ins[1], Instruction::Jump(_)));
}

#[test]
fn continue_inside_while_targets_loop_start() {
    let parsed = parse_source("fn ct() { while true { continue; } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(ins[0], Instruction::LoadTrue);
    assert!(matches!(ins[1], Instruction::JumpIfFalse(_)));
    assert!(matches!(ins[2], Instruction::Jump(_)));
}

#[test]
fn short_circuit_and_uses_jump_if_false_pattern() {
    let parsed = parse_source("fn a() { true && false }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Layout:
    //   load_true             ; lhs
    //   jump_if_false short
    //   load_false            ; rhs
    //   jump end
    // short:
    //   load_false
    // end:
    //   return
    assert_eq!(ins[0], Instruction::LoadTrue);
    assert!(matches!(ins[1], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[2], Instruction::LoadFalse);
    assert!(matches!(ins[3], Instruction::Jump(_)));
    assert_eq!(ins[4], Instruction::LoadFalse);
    assert_eq!(ins[5], Instruction::Return);
}

#[test]
fn short_circuit_or_uses_load_true_branch() {
    let parsed = parse_source("fn o() { false || true }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Layout:
    //   load_false            ; lhs
    //   jump_if_false try_rhs
    //   load_true             ; lhs was true -> result true
    //   jump end
    // try_rhs:
    //   load_true             ; rhs
    // end:
    //   return
    assert_eq!(ins[0], Instruction::LoadFalse);
    assert!(matches!(ins[1], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[2], Instruction::LoadTrue);
    assert!(matches!(ins[3], Instruction::Jump(_)));
    assert_eq!(ins[4], Instruction::LoadTrue);
    assert_eq!(ins[5], Instruction::Return);
}

// --- S5b.3: function calls + parameters ---

fn decode_fn_table(module: &Module) -> FunctionTable {
    let s = module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    FunctionTable::decode(&s.payload).unwrap()
}

#[test]
fn call_to_known_fn_emits_call_with_correct_argc_and_fn_idx() {
    let src = "fn add(x: i32, y: i32) -> i32 { x + y }\nfn main() { add(1, 2) }\n";
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let table = decode_fn_table(&out.module);
    assert_eq!(table.entries.len(), 2);
    // `add` is declared first → index 0; `main` → index 1.
    assert_eq!(table.entries[0].name, "add");
    assert_eq!(table.entries[1].name, "main");
    let main_ins = decode(&table.entries[1].code).unwrap();
    // load_const 0 (1); load_const 1 (2); call 0, 2; return
    assert_eq!(main_ins[0], Instruction::LoadConst(0));
    assert_eq!(main_ins[1], Instruction::LoadConst(1));
    assert_eq!(main_ins[2], Instruction::Call { fn_idx: 0, argc: 2 });
    assert_eq!(main_ins[3], Instruction::Return);
}

#[test]
fn forward_call_resolves_via_two_pass_index_build() {
    // `main` is declared before `helper`; the call to `helper` must
    // still resolve to the correct (later) fn_idx.
    let src = "fn main() { helper() }\nfn helper() { 99 }\n";
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let table = decode_fn_table(&out.module);
    let main_ins = decode(&table.entries[0].code).unwrap();
    assert_eq!(main_ins[0], Instruction::Call { fn_idx: 1, argc: 0 });
}

#[test]
fn unknown_callee_is_reported() {
    let parsed = parse_source("fn main() { missing(1) }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    match &out.errors[0].kind {
        capy_emitter::EmitErrorKind::UnknownFunction { name } => {
            assert_eq!(name, "missing");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn duplicate_function_is_reported_and_first_kept() {
    let parsed = parse_source("fn dup() { 1 }\nfn dup() { 2 }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::DuplicateFunction { .. }
    ));
    let table = decode_fn_table(&out.module);
    // One reserved slot per unique name; the duplicate is skipped.
    assert_eq!(table.entries.len(), 1);
    assert_eq!(table.entries[0].name, "dup");
}

#[test]
fn path_callee_is_unsupported_in_s5b3() {
    let parsed = parse_source("fn main() { foo::bar() }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::UnsupportedCallee { .. }
    ));
}

#[test]
fn module_round_trips_through_parse() {
    let parsed = parse_source("fn add() { 1 + 2 }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty());
    let bytes = out.module.serialize();
    let parsed_module = Module::parse(&bytes).expect("Module::parse must succeed");
    // The serialised → parsed Module is equal to the original.
    assert_eq!(parsed_module, out.module);
}

#[test]
fn module_emits_debug_section_with_source_spans() {
    // `fn main() { 1 + 2 }` — verify the emitter shipped a `Debug`
    // section whose entries cover the function's bytecode offsets
    // and point back at the source byte ranges. Spot-check that the
    // first entry covers the start of the function body and that
    // each entry's source span is non-empty.
    let src = "fn main() { 1 + 2 }\n";
    let parsed = parse_source(src);
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);

    let debug_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == capy_bytecode::SectionTag::Debug)
        .expect("emitter should ship a Debug section once it has entries");
    let debug = capy_bytecode::DebugInfo::decode(&debug_section.payload)
        .expect("Debug section round-trips through DebugInfo::decode");
    assert!(!debug.entries.is_empty());
    // bytecode_offsets must be strictly non-decreasing.
    for w in debug.entries.windows(2) {
        assert!(w[0].bytecode_offset <= w[1].bytecode_offset);
    }
    // Each entry's source span must point inside the source.
    let src_len = src.len() as u32;
    for e in &debug.entries {
        assert!(e.source_start <= e.source_end);
        assert!(e.source_end <= src_len);
    }
}

#[test]
fn module_omits_debug_section_for_an_empty_source() {
    // No functions → no debug entries → no `Debug` section. This
    // guarantees byte-identical round-trips with the pre-S3-followup
    // emitter output for empty inputs.
    let parsed = parse_source("");
    let out = emit(&parsed.source);
    // The empty source produces a "top-level must be item" diagnostic
    // but no functions and no debug entries.
    assert!(out
        .module
        .sections
        .iter()
        .all(|s| s.tag != capy_bytecode::SectionTag::Debug));
}

// --- S7: import lowering -----------------------------------------------

#[test]
fn imported_call_lowers_to_host_call() {
    // Bare `import time::now;` registers a host import at index 0
    // and the call site `now()` lowers to HostCall, not Call.
    let parsed = parse_source("import time::now;\nfn main() { now() }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(
        ins,
        vec![
            Instruction::HostCall {
                import_idx: 0,
                argc: 0,
            },
            Instruction::Return,
        ]
    );
}

#[test]
fn imported_call_with_args_forwards_argc() {
    // Arguments are pushed left-to-right; argc matches the source.
    let parsed = parse_source("import log::info;\nfn main() { info(\"hi\") }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Str("hi".into())]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::HostCall {
                import_idx: 0,
                argc: 1,
            },
            Instruction::Return,
        ]
    );
}

#[test]
fn import_alias_renames_callable_but_keeps_wire_symbol() {
    // `import log::info as say` lets the source say `say("hi")`
    // while the bytecode `Imports` section still binds against
    // (module="log", symbol="info").
    let parsed = parse_source("import log::info as say;\nfn main() { say(\"hi\") }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert!(matches!(
        ins[1],
        Instruction::HostCall {
            import_idx: 0,
            argc: 1,
        }
    ));
    let imports_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Imports)
        .expect("imports section");
    let imports = ImportTable::decode(&imports_section.payload).expect("decode imports");
    assert_eq!(imports.entries[0].module, "log");
    assert_eq!(imports.entries[0].symbol, "info");
}

#[test]
fn local_fn_shadows_import_of_same_name() {
    // When both `import x::foo;` and `fn foo() {}` declare the same
    // callable name, the local `fn` wins (Rust's `use` precedence) so
    // `foo()` lowers to `Call`, not `HostCall`. The import is still
    // emitted into the `Imports` section so the host bridge surface
    // remains observable to tooling.
    let parsed = parse_source("import x::foo;\nfn foo() {}\nfn main() { foo() }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let consts_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Consts)
        .expect("consts section");
    let _ = ConstPool::decode(&consts_section.payload).expect("decode consts");
    let functions_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Functions)
        .expect("functions section");
    let functions = FunctionTable::decode(&functions_section.payload).expect("decode functions");
    let main_idx = functions
        .entries
        .iter()
        .position(|f| f.name == "main")
        .expect("main fn");
    let main_ins = decode(&functions.entries[main_idx].code).expect("decode main");
    // The call site must use Call (to the local `foo`), not HostCall.
    assert!(matches!(main_ins[0], Instruction::Call { argc: 0, .. }));
}

#[test]
fn duplicate_import_name_is_reported_first_wins() {
    // Two imports collapsing to the same callable name (here both end
    // in `now`) emit a `DuplicateImport` error for the second; only
    // the first reaches the `Imports` section.
    let parsed = parse_source("import time::now;\nimport other::now;\nfn nop() {}\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::DuplicateImport { ref name }
            if name == "now"
    ));
    let imports_section = out
        .module
        .sections
        .iter()
        .find(|s| s.tag == SectionTag::Imports)
        .expect("imports section");
    let imports = ImportTable::decode(&imports_section.payload).expect("decode imports");
    assert_eq!(imports.entries.len(), 1);
    assert_eq!(imports.entries[0].module, "time");
    assert_eq!(imports.entries[0].symbol, "now");
}

#[test]
fn unknown_callee_is_still_reported() {
    // A bare `nope()` with no matching fn or import must still surface
    // `UnknownFunction` — host_call lowering does not silently mask
    // unresolved callees.
    let parsed = parse_source("fn main() { nope() }\n");
    let out = emit(&parsed.source);
    assert_eq!(out.errors.len(), 1);
    assert!(matches!(
        out.errors[0].kind,
        capy_emitter::EmitErrorKind::UnknownFunction { ref name }
            if name == "nope"
    ));
}

// --- S2.2b → emitter: match lowering -----------------------------------

#[test]
fn match_literal_arms_lower_to_eq_and_jump_chain() {
    // `match 1 { 1 => 10, _ => 0 }` lowers to:
    //   load_const 1            ; scrutinee
    //   store_local 0           ; scrut slot
    //   load_local 0
    //   load_const 1            ; literal
    //   eq
    //   jump_if_false next_0
    //   load_const 10           ; arm 0 body
    //   jump end
    //   next_0:                 ; arm 1 (wildcard)
    //   load_const 0
    //   jump end
    //   next_1:
    //   load_none               ; fall-through
    //   end:
    //   return
    let parsed = parse_source("fn main() { match 1 { 1 => 10, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(table.entries[0].locals_count, 1, "match allocates one slot");
    // The constant pool is interned, so the literal `1` is reused for
    // both the scrutinee LoadConst and the pattern LoadConst.
    assert_eq!(
        consts.entries,
        vec![Constant::Int(1), Constant::Int(10), Constant::Int(0)]
    );
    // Spot-check the dispatch shape: scrutinee store, then the
    // literal-pattern comparison.
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::StoreLocal(0));
    assert_eq!(ins[2], Instruction::LoadLocal(0));
    assert_eq!(ins[3], Instruction::LoadConst(0));
    assert_eq!(ins[4], Instruction::Eq);
    assert!(matches!(ins[5], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[6], Instruction::LoadConst(1)); // body 10
    assert!(matches!(ins[7], Instruction::Jump(_)));
    // Wildcard arm: no pattern test, no JumpIfFalse, straight to body.
    assert_eq!(ins[8], Instruction::LoadConst(2)); // body 0
    assert!(matches!(ins[9], Instruction::Jump(_)));
    // Fall-through (no arm matched) — verifier-safe.
    assert_eq!(ins[10], Instruction::LoadNone);
    // Tail of the function body Returns the match's value.
    assert_eq!(ins[11], Instruction::Return);
}

#[test]
fn match_ident_pattern_binds_and_uses_in_body() {
    // `n` matches anything and binds the scrutinee into a fresh local
    // for the arm body. The body `n + 1` reads the binding via
    // LoadLocal.
    let parsed = parse_source("fn main() { match 5 { n => n + 1 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, _consts, ins) = decode_first_fn(&out.module);
    // Locals: slot 0 = scrutinee, slot 1 = binding `n`.
    assert_eq!(table.entries[0].locals_count, 2);
    // Scrutinee push + store.
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::StoreLocal(0));
    // Ident pattern emits no test; the binding is: load scrut, store n.
    assert_eq!(ins[2], Instruction::LoadLocal(0));
    assert_eq!(ins[3], Instruction::StoreLocal(1));
    // Body: n + 1.
    assert_eq!(ins[4], Instruction::LoadLocal(1));
    assert_eq!(ins[5], Instruction::LoadConst(1));
    assert_eq!(ins[6], Instruction::Add);
}

#[test]
fn match_guard_branches_to_next_arm() {
    // `match 3 { n if n > 0 => 1, _ => 0 }` should evaluate the guard
    // after binding `n`; a falsy guard must skip to the next arm.
    let parsed = parse_source("fn main() { match 3 { n if n > 0 => 1, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // The guard's JumpIfFalse must appear between the binding and the
    // body. We scan for two JumpIfFalse / Jump pairs (one per arm) to
    // confirm the dispatch shape; we accept any concrete offset.
    let mut jif_count = 0;
    let mut jmp_count = 0;
    for ins in &ins {
        match ins {
            Instruction::JumpIfFalse(_) => jif_count += 1,
            Instruction::Jump(_) => jmp_count += 1,
            _ => {}
        }
    }
    // Arm 1 contributes 1 JumpIfFalse (guard); arm 0 contributes 0
    // because Ident never tests. Both arms contribute 1 Jump to end.
    assert!(
        jif_count >= 1,
        "expected at least one JumpIfFalse for the guard"
    );
    assert!(jmp_count >= 2, "expected one Jump-to-end per arm");
}

#[test]
fn match_arm_bindings_do_not_leak_across_arms() {
    // Two arms both bind `n`. After arm 0 finishes lowering, the
    // emitter restores the locals map; arm 1's `n` therefore gets a
    // fresh slot rather than tripping `DuplicateLocal`. The emit
    // pipeline must succeed without errors.
    let parsed = parse_source("fn main() { match 1 { n => n, n => n } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
}

#[test]
fn match_struct_pattern_lowers_with_get_tag_and_get_field() {
    let parsed = parse_source(
        "struct Point { x: Int, y: Int }\n\
         fn main() { let p = Point { x: 7, y: 9 }; match p { Point { x } => x, _ => 0 } }\n",
    );
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert!(
        ins.iter().any(|i| matches!(i, Instruction::GetTag)),
        "expected get_tag in {ins:?}"
    );
    assert!(
        ins.iter().any(|i| matches!(i, Instruction::GetField(0))),
        "expected get_field 0 in {ins:?}"
    );
}

#[test]
fn match_range_pattern_lowers_to_bounds_checks() {
    // `match 5 { 0..=9 => 1, _ => 0 }` lowers the range to two bounds
    // checks (`ge` + `le`) each followed by a JumpIfFalse to the next
    // arm. The inclusive form uses `le`; the exclusive form `..` uses
    // `lt` (asserted by the next test).
    let parsed = parse_source("fn main() { match 5 { 0..=9 => 1, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Scrutinee push + store, then the range lower bound, then upper.
    assert_eq!(ins[0], Instruction::LoadConst(0));
    assert_eq!(ins[1], Instruction::StoreLocal(0));
    assert_eq!(ins[2], Instruction::LoadLocal(0));
    // 0 is interned (idx 1, since `5` got idx 0).
    assert!(matches!(ins[3], Instruction::LoadConst(_)));
    assert_eq!(ins[4], Instruction::Ge);
    assert!(matches!(ins[5], Instruction::JumpIfFalse(_)));
    assert_eq!(ins[6], Instruction::LoadLocal(0));
    assert!(matches!(ins[7], Instruction::LoadConst(_)));
    assert_eq!(ins[8], Instruction::Le);
    assert!(matches!(ins[9], Instruction::JumpIfFalse(_)));
}

#[test]
fn match_exclusive_range_uses_lt_for_upper_bound() {
    // `0..9` (exclusive) — the upper-bound comparison must be Lt.
    let parsed = parse_source("fn main() { match 5 { 0..9 => 1, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(ins[4], Instruction::Ge);
    assert_eq!(ins[8], Instruction::Lt);
}

#[test]
fn match_or_pattern_chains_alts_with_shared_body() {
    // `match 2 { 1 | 2 | 3 => 100, _ => 0 }` — each non-last alt
    // jumps to a shared body label on success; the last alt's
    // failure falls through to the next arm. We confirm the dispatch
    // shape by counting comparison + jump instructions.
    let parsed = parse_source("fn main() { match 2 { 1 | 2 | 3 => 100, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    // Three Eq tests (one per alt), three JumpIfFalse (one per alt),
    // two Jump-to-body (non-last alts), one Jump-to-end (arm 0 body),
    // one Jump-to-end (arm 1 body) = 4 total Jumps in arm 0+1 region.
    let eq_count = ins.iter().filter(|i| matches!(i, Instruction::Eq)).count();
    let jif_count = ins
        .iter()
        .filter(|i| matches!(i, Instruction::JumpIfFalse(_)))
        .count();
    assert_eq!(eq_count, 3, "one Eq per or-pattern alt");
    assert_eq!(jif_count, 3, "one JumpIfFalse per or-pattern alt");
}

#[test]
fn match_or_pattern_rejects_ident_alt() {
    // Identifier-binding alts in or-patterns are deferred to a later
    // slice (binding-merging is non-trivial). The emitter must
    // surface a typed `UnsupportedFeature` rather than silently miss
    // the binding semantics.
    let parsed = parse_source("fn main() { match 1 { 1 | x => 0, _ => 0 } }\n");
    let out = emit(&parsed.source);
    assert!(!out.errors.is_empty());
    assert!(out.errors.iter().any(|e| matches!(
        &e.kind,
        capy_emitter::EmitErrorKind::UnsupportedFeature { what }
            if what.contains("identifier binding in or-pattern")
    )));
}

#[test]
fn assignment_lowers_to_store_local_then_none() {
    let parsed = parse_source("fn main() { let x = 1; x = 2; }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(1), Constant::Int(2)]);
    assert_eq!(table.entries[0].locals_count, 1);
    assert_eq!(
        ins,
        vec![
            // let x = 1;
            Instruction::LoadConst(0),
            Instruction::StoreLocal(0),
            // x = 2;  — assignment stores then evaluates to None, which the
            // expression statement immediately pops.
            Instruction::LoadConst(1),
            Instruction::StoreLocal(0),
            Instruction::LoadNone,
            Instruction::Pop,
            // block has no tail.
            Instruction::LoadNone,
            Instruction::Return,
        ]
    );
}

#[test]
fn assignment_to_non_local_is_rejected() {
    // The left-hand side `1` is not an assignable place (E0021).
    let parsed = parse_source("fn main() { 1 = 2; }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.iter().any(|e| e.code() == "E0021"));
}

#[test]
fn assignment_to_undeclared_local_is_rejected() {
    // `y` was never bound with `let` (E0003).
    let parsed = parse_source("fn main() { y = 1; }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.iter().any(|e| e.code() == "E0003"));
}

#[test]
fn bitwise_and_lowers_to_band_opcode() {
    let parsed = parse_source("fn main() { 6 & 3 }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(6), Constant::Int(3)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::BitAnd,
            Instruction::Return,
        ]
    );
}

#[test]
fn bitwise_not_lowers_to_bnot_opcode() {
    let parsed = parse_source("fn main() { ~5 }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(5)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::BitNot,
            Instruction::Return,
        ]
    );
}

// --- S6.3b: enum construction + match lowering -------------------------

#[test]
fn unit_variant_lowers_to_make_aggregate() {
    // `Color::Blue` is the third declared variant → tag 2, no fields.
    let parsed = parse_source("enum Color { Red, Green, Blue }\nfn main() { Color::Blue }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(
        ins,
        vec![
            Instruction::MakeAggregate {
                tag: 2,
                field_count: 0,
            },
            Instruction::Return,
        ]
    );
}

#[test]
fn tuple_variant_lowers_to_args_then_make_aggregate() {
    // `Box::Val(9)` pushes the field then builds a 1-field aggregate.
    let parsed = parse_source("enum Box { Val(Int), Empty }\nfn main() { Box::Val(9) }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(9)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0),
            Instruction::MakeAggregate {
                tag: 0,
                field_count: 1,
            },
            Instruction::Return,
        ]
    );
}

#[test]
fn enum_declaration_emits_no_code_and_no_errors() {
    // The enum contributes no function entry; only `main` is emitted.
    let parsed = parse_source("enum E { A, B }\nfn main() {}\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (table, _consts, ins) = decode_first_fn(&out.module);
    assert_eq!(table.entries.len(), 1);
    assert_eq!(table.entries[0].name, "main");
    assert_eq!(ins, vec![Instruction::LoadNone, Instruction::Return]);
}

#[test]
fn match_tuple_variant_lowers_with_get_tag_and_get_field() {
    let parsed = parse_source(
        "enum Box { Val(Int), Empty }\n\
         fn main() { match Box::Val(5) { Box::Val(x) => x, Box::Empty => 0 } }\n",
    );
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, _consts, ins) = decode_first_fn(&out.module);
    assert!(
        ins.iter().any(|i| matches!(i, Instruction::GetTag)),
        "expected get_tag in {ins:?}"
    );
    assert!(
        ins.iter().any(|i| matches!(i, Instruction::GetField(0))),
        "expected get_field 0 in {ins:?}"
    );
    assert!(
        ins.iter()
            .any(|i| matches!(i, Instruction::MakeAggregate { .. })),
        "expected make_aggregate in {ins:?}"
    );
}

#[test]
fn struct_literal_emits_fields_in_declaration_order() {
    // The literal lists `y` first, but the emitter must push `x`'s value
    // (10) before `y`'s value (20) so field 0 is `x` (S6.3c). The const
    // pool is interned in emission order, so x's constant gets index 0.
    let parsed =
        parse_source("struct Point { x: Int, y: Int }\nfn main() { Point { y: 20, x: 10 } }\n");
    let out = emit(&parsed.source);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let (_table, consts, ins) = decode_first_fn(&out.module);
    assert_eq!(consts.entries, vec![Constant::Int(10), Constant::Int(20)]);
    assert_eq!(
        ins,
        vec![
            Instruction::LoadConst(0), // x = 10 (declared first)
            Instruction::LoadConst(1), // y = 20
            Instruction::MakeAggregate {
                tag: 0,
                field_count: 2,
            },
            Instruction::Return,
        ]
    );
}
