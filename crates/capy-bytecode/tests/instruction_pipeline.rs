//! End-to-end: build an instruction stream, encode into a
//! [`Function`], wrap in a [`FunctionTable`] section payload, embed in
//! a [`Module`], serialise, parse, decode the function payload, decode
//! the instruction stream and compare against the original.

use capy_bytecode::{
    decode, encode, Function, FunctionTable, Instruction, Module, Section, SectionTag,
};

#[test]
fn instructions_round_trip_through_module() {
    // A tiny "fn add() { 1 + 2 }" lowering:
    //   load_const 0  ; push 1
    //   load_const 1  ; push 2
    //   add
    //   return
    let original = vec![
        Instruction::LoadConst(0),
        Instruction::LoadConst(1),
        Instruction::Add,
        Instruction::Return,
    ];

    let function = Function {
        name: "add".to_string(),
        locals_count: 0,
        code: encode(&original),
    };
    let function_table = FunctionTable {
        entries: vec![function],
    };
    let module = Module::new(
        0,
        vec![Section::new(SectionTag::Functions, function_table.encode())],
    );

    // Serialise + parse the container.
    let bytes = module.serialize();
    let parsed = Module::parse(&bytes).expect("parse must succeed");

    // Walk back: section -> typed table -> function -> instructions.
    assert_eq!(parsed.sections.len(), 1);
    assert_eq!(parsed.sections[0].tag, SectionTag::Functions);
    let parsed_table = FunctionTable::decode(&parsed.sections[0].payload).unwrap();
    assert_eq!(parsed_table.entries.len(), 1);
    let parsed_fn = &parsed_table.entries[0];
    assert_eq!(parsed_fn.name, "add");
    let parsed_instructions = decode(&parsed_fn.code).expect("instruction decode must succeed");
    assert_eq!(parsed_instructions, original);
}

#[test]
fn jump_offsets_survive_container_round_trip() {
    // Pattern equivalent to "if x { 1 } else { 2 }" — without the cond
    // expression, just the branch shape:
    //
    //   0000  load_const 0   (cond pushed by caller)
    //   0005  jump_if_false  +10   ; -> 000f (else)
    //   000a  load_const 1         ; then
    //   000f  jump           +5    ; -> 0014 (end)
    //   0014  load_const 2         ; else
    //   0019  return
    let stream = vec![
        Instruction::LoadConst(0),
        Instruction::JumpIfFalse(10),
        Instruction::LoadConst(1),
        Instruction::Jump(5),
        Instruction::LoadConst(2),
        Instruction::Return,
    ];
    let encoded = encode(&stream);
    // Wrap and round-trip through the module.
    let table = FunctionTable {
        entries: vec![Function {
            name: "branch".to_string(),
            locals_count: 0,
            code: encoded,
        }],
    };
    let module = Module::new(0, vec![Section::new(SectionTag::Functions, table.encode())]);
    let bytes = module.serialize();
    let parsed = Module::parse(&bytes).unwrap();
    let parsed_table = FunctionTable::decode(&parsed.sections[0].payload).unwrap();
    let parsed_stream = decode(&parsed_table.entries[0].code).unwrap();
    assert_eq!(parsed_stream, stream);
}
