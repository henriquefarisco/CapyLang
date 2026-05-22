//! End-to-end round trip: typed payloads → `Section` bytes → `Module`
//! bytes → parsed `Module` → typed payloads. Validates that S4 framing
//! and S4b per-section encoders compose deterministically.

use capy_bytecode::{
    ConstPool, Constant, DebugEntry, DebugInfo, Function, FunctionTable, Import, ImportTable,
    Module, Section, SectionTag,
};

#[test]
fn full_module_round_trip() {
    let consts = ConstPool {
        entries: vec![
            Constant::Int(42),
            Constant::Float(3.125),
            Constant::Str("hello".to_string()),
        ],
    };
    let functions = FunctionTable {
        entries: vec![
            Function {
                name: "main".to_string(),
                locals_count: 2,
                code: vec![0x00, 0x01, 0x02, 0xFF],
            },
            Function {
                name: "helper".to_string(),
                locals_count: 0,
                code: vec![0x10, 0x20],
            },
        ],
    };
    let imports = ImportTable {
        entries: vec![
            Import {
                module: "time".to_string(),
                symbol: "now".to_string(),
            },
            Import {
                module: "log".to_string(),
                symbol: "info".to_string(),
            },
        ],
    };
    let debug = DebugInfo {
        entries: vec![
            DebugEntry {
                bytecode_offset: 0,
                source_start: 0,
                source_end: 4,
            },
            DebugEntry {
                bytecode_offset: 4,
                source_start: 5,
                source_end: 10,
            },
        ],
    };

    let module = Module::new(
        7,
        vec![
            Section::new(SectionTag::Consts, consts.encode()),
            Section::new(SectionTag::Functions, functions.encode()),
            Section::new(SectionTag::Imports, imports.encode()),
            Section::new(SectionTag::Debug, debug.encode()),
        ],
    );

    let bytes = module.serialize();
    // Deterministic serialisation.
    assert_eq!(bytes, module.serialize());

    let parsed = Module::parse(&bytes).expect("parse must succeed");
    assert_eq!(parsed.abi_version, 7);
    assert_eq!(parsed.sections.len(), 4);

    // Decode each payload back into typed structures and compare.
    let parsed_consts = ConstPool::decode(&parsed.sections[0].payload).unwrap();
    let parsed_functions = FunctionTable::decode(&parsed.sections[1].payload).unwrap();
    let parsed_imports = ImportTable::decode(&parsed.sections[2].payload).unwrap();
    let parsed_debug = DebugInfo::decode(&parsed.sections[3].payload).unwrap();

    assert_eq!(parsed_consts, consts);
    assert_eq!(parsed_functions, functions);
    assert_eq!(parsed_imports, imports);
    assert_eq!(parsed_debug, debug);
}

#[test]
fn typed_payloads_are_independent_of_section_framing() {
    // Encoding a pool then wrapping it in a section produces the same
    // bytes as decoding the wrapped section's payload back into a pool.
    let pool = ConstPool {
        entries: vec![Constant::Int(1), Constant::Int(2)],
    };
    let payload = pool.encode();
    let section = Section::new(SectionTag::Consts, payload.clone());
    assert_eq!(section.payload, payload);
    let decoded = ConstPool::decode(&section.payload).unwrap();
    assert_eq!(decoded, pool);
}
