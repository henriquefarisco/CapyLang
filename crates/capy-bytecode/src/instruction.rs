//! High-level [`Instruction`] enum and the codec that round-trips it
//! against the raw bytes stored in a [`Function::code`](crate::Function).
//!
//! Encoding rules: every opcode is exactly 1 byte; the optional
//! immediate is either 4 little-endian unsigned bytes
//! ([`Imm::U32`](crate::opcode::Imm::U32)) or 4 little-endian signed
//! bytes ([`Imm::I32`](crate::opcode::Imm::I32)). The decoder validates
//! the opcode byte and rejects truncated immediates.
//!
//! `Jump` / `JumpIfFalse` carry a signed offset relative to the byte
//! that follows the full instruction (PC after decoding the immediate).

#![forbid(unsafe_code)]

use crate::cursor::Cursor;
use crate::error::BytecodeError;
use crate::opcode::Opcode;

/// A decoded v0 instruction. Variants carrying immediates expose them as
/// typed Rust integers so emitters and disassemblers can manipulate them
/// without re-parsing byte slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instruction {
    Nop,
    Pop,
    LoadConst(u32),
    LoadTrue,
    LoadFalse,
    LoadNone,
    LoadLocal(u32),
    StoreLocal(u32),
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    Jump(i32),
    JumpIfFalse(i32),
    /// Call into another function in the same module.
    ///
    /// Operands: `fn_idx` selects the entry in the module's
    /// `FunctionTable`; `argc` is the number of values popped from the
    /// caller's operand stack and used to initialise the callee's
    /// `locals[0..argc]` (slot `0` = first argument, slot `argc-1` =
    /// last argument). The callee's remaining local slots (if any) are
    /// initialised to `None`.
    Call {
        fn_idx: u32,
        argc: u32,
    },
    Return,
    /// Call out into the host adapter.
    ///
    /// Operands: `import_idx` selects the entry in the module's
    /// `ImportTable`; `argc` is the number of values popped from the
    /// operand stack and passed to the registered host function. The
    /// host returns exactly one value, which is pushed back onto the
    /// operand stack. Stack-effect: pops `argc`, pushes 1.
    HostCall {
        import_idx: u32,
        argc: u32,
    },
}

impl Instruction {
    /// Opcode byte for this instruction (without the immediate).
    #[must_use]
    pub const fn opcode(self) -> Opcode {
        match self {
            Self::Nop => Opcode::Nop,
            Self::Pop => Opcode::Pop,
            Self::LoadConst(_) => Opcode::LoadConst,
            Self::LoadTrue => Opcode::LoadTrue,
            Self::LoadFalse => Opcode::LoadFalse,
            Self::LoadNone => Opcode::LoadNone,
            Self::LoadLocal(_) => Opcode::LoadLocal,
            Self::StoreLocal(_) => Opcode::StoreLocal,
            Self::Add => Opcode::Add,
            Self::Sub => Opcode::Sub,
            Self::Mul => Opcode::Mul,
            Self::Div => Opcode::Div,
            Self::Mod => Opcode::Mod,
            Self::Neg => Opcode::Neg,
            Self::Eq => Opcode::Eq,
            Self::Ne => Opcode::Ne,
            Self::Lt => Opcode::Lt,
            Self::Le => Opcode::Le,
            Self::Gt => Opcode::Gt,
            Self::Ge => Opcode::Ge,
            Self::Not => Opcode::Not,
            Self::Jump(_) => Opcode::Jump,
            Self::JumpIfFalse(_) => Opcode::JumpIfFalse,
            Self::Call { .. } => Opcode::Call,
            Self::Return => Opcode::Return,
            Self::HostCall { .. } => Opcode::HostCall,
        }
    }

    /// Encoded byte width (opcode + immediate).
    #[must_use]
    pub const fn width(self) -> usize {
        self.opcode().immediate().width()
    }

    /// Appends this instruction's encoded bytes to `out`.
    pub fn encode_into(self, out: &mut Vec<u8>) {
        out.push(self.opcode().as_byte());
        match self {
            Self::LoadConst(v) | Self::LoadLocal(v) | Self::StoreLocal(v) => {
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Jump(v) | Self::JumpIfFalse(v) => {
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Call { fn_idx, argc } => {
                out.extend_from_slice(&fn_idx.to_le_bytes());
                out.extend_from_slice(&argc.to_le_bytes());
            }
            Self::HostCall { import_idx, argc } => {
                out.extend_from_slice(&import_idx.to_le_bytes());
                out.extend_from_slice(&argc.to_le_bytes());
            }
            _ => {}
        }
    }
}

/// Encodes a list of instructions into a fresh byte buffer.
#[must_use]
pub fn encode(instructions: &[Instruction]) -> Vec<u8> {
    let mut out = Vec::new();
    for ins in instructions {
        ins.encode_into(&mut out);
    }
    out
}

/// Decodes every instruction in `code`, returning them in source order.
///
/// Fail-closed: unknown opcode bytes, truncated immediates and trailing
/// bytes are all rejected with
/// [`BytecodeError::MalformedInstruction`].
pub fn decode(code: &[u8]) -> Result<Vec<Instruction>, BytecodeError> {
    let mut cursor = Cursor::new(code);
    let mut out = Vec::new();
    while !cursor.is_empty() {
        let op_pos = cursor.pos();
        let byte = cursor
            .read_u8()
            .ok_or(BytecodeError::MalformedInstruction {
                offset: op_pos,
                reason: "truncated opcode",
            })?;
        let op = Opcode::from_byte(byte).ok_or(BytecodeError::MalformedInstruction {
            offset: op_pos,
            reason: "unknown opcode",
        })?;
        let ins = match op {
            Opcode::Nop => Instruction::Nop,
            Opcode::Pop => Instruction::Pop,
            Opcode::LoadConst => Instruction::LoadConst(read_u32(&mut cursor, op_pos)?),
            Opcode::LoadTrue => Instruction::LoadTrue,
            Opcode::LoadFalse => Instruction::LoadFalse,
            Opcode::LoadNone => Instruction::LoadNone,
            Opcode::LoadLocal => Instruction::LoadLocal(read_u32(&mut cursor, op_pos)?),
            Opcode::StoreLocal => Instruction::StoreLocal(read_u32(&mut cursor, op_pos)?),
            Opcode::Add => Instruction::Add,
            Opcode::Sub => Instruction::Sub,
            Opcode::Mul => Instruction::Mul,
            Opcode::Div => Instruction::Div,
            Opcode::Mod => Instruction::Mod,
            Opcode::Neg => Instruction::Neg,
            Opcode::Eq => Instruction::Eq,
            Opcode::Ne => Instruction::Ne,
            Opcode::Lt => Instruction::Lt,
            Opcode::Le => Instruction::Le,
            Opcode::Gt => Instruction::Gt,
            Opcode::Ge => Instruction::Ge,
            Opcode::Not => Instruction::Not,
            Opcode::Jump => Instruction::Jump(read_i32(&mut cursor, op_pos)?),
            Opcode::JumpIfFalse => Instruction::JumpIfFalse(read_i32(&mut cursor, op_pos)?),
            Opcode::Call => {
                let fn_idx = read_u32(&mut cursor, op_pos)?;
                let argc = read_u32(&mut cursor, op_pos)?;
                Instruction::Call { fn_idx, argc }
            }
            Opcode::Return => Instruction::Return,
            Opcode::HostCall => {
                let import_idx = read_u32(&mut cursor, op_pos)?;
                let argc = read_u32(&mut cursor, op_pos)?;
                Instruction::HostCall { import_idx, argc }
            }
        };
        out.push(ins);
    }
    Ok(out)
}

fn read_u32(cursor: &mut Cursor<'_>, op_pos: usize) -> Result<u32, BytecodeError> {
    cursor
        .read_u32_le()
        .ok_or(BytecodeError::MalformedInstruction {
            offset: op_pos,
            reason: "truncated u32 immediate",
        })
}

fn read_i32(cursor: &mut Cursor<'_>, op_pos: usize) -> Result<i32, BytecodeError> {
    let bytes = cursor
        .read_bytes(4)
        .ok_or(BytecodeError::MalformedInstruction {
            offset: op_pos,
            reason: "truncated i32 immediate",
        })?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(bytes);
    Ok(i32::from_le_bytes(buf))
}

/// Renders a decoded instruction stream as one mnemonic per line, with
/// byte offsets in the first column. Used by debug tooling.
///
/// ```text
/// 0000  load_const  0
/// 0005  load_const  1
/// 000a  add
/// 000b  return
/// ```
#[must_use]
pub fn disassemble_text(instructions: &[Instruction]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let mut offset: usize = 0;
    for ins in instructions {
        let _ = write!(out, "{offset:04x}  {}", ins.opcode().mnemonic());
        match *ins {
            Instruction::LoadConst(v) | Instruction::LoadLocal(v) | Instruction::StoreLocal(v) => {
                let _ = write!(out, "  {v}");
            }
            Instruction::Jump(v) | Instruction::JumpIfFalse(v) => {
                let _ = write!(out, "  {v}");
            }
            Instruction::Call { fn_idx, argc } => {
                let _ = write!(out, "  {fn_idx}, {argc}");
            }
            Instruction::HostCall { import_idx, argc } => {
                let _ = write!(out, "  {import_idx}, {argc}");
            }
            _ => {}
        }
        out.push('\n');
        offset += ins.width();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_round_trip() {
        let stream = vec![
            Instruction::Nop,
            Instruction::Pop,
            Instruction::LoadConst(7),
            Instruction::LoadTrue,
            Instruction::LoadFalse,
            Instruction::LoadNone,
            Instruction::LoadLocal(0),
            Instruction::StoreLocal(1),
            Instruction::Add,
            Instruction::Sub,
            Instruction::Mul,
            Instruction::Div,
            Instruction::Mod,
            Instruction::Neg,
            Instruction::Eq,
            Instruction::Ne,
            Instruction::Lt,
            Instruction::Le,
            Instruction::Gt,
            Instruction::Ge,
            Instruction::Not,
            Instruction::Jump(-4),
            Instruction::JumpIfFalse(8),
            Instruction::Call { fn_idx: 7, argc: 2 },
            Instruction::Return,
            Instruction::HostCall {
                import_idx: 3,
                argc: 1,
            },
        ];
        let bytes = encode(&stream);
        let parsed = decode(&bytes).unwrap();
        assert_eq!(parsed, stream);
    }

    #[test]
    fn arithmetic_program_encodes_predictably() {
        // load_const 0; load_const 1; add; return
        let stream = vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ];
        let bytes = encode(&stream);
        assert_eq!(
            bytes,
            vec![
                0x10, 0, 0, 0, 0, // LoadConst 0
                0x10, 1, 0, 0, 0,    // LoadConst 1
                0x30, // Add
                0x81, // Return
            ]
        );
    }

    #[test]
    fn empty_code_decodes_to_empty_vec() {
        assert!(decode(&[]).unwrap().is_empty());
    }

    #[test]
    fn rejects_unknown_opcode() {
        let err = decode(&[0xAB]).unwrap_err();
        match err {
            BytecodeError::MalformedInstruction { reason, .. } => {
                assert_eq!(reason, "unknown opcode");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_u32_immediate() {
        // LoadConst opcode followed by only 2 bytes (need 4).
        let err = decode(&[0x10, 0x00, 0x00]).unwrap_err();
        match err {
            BytecodeError::MalformedInstruction { reason, .. } => {
                assert_eq!(reason, "truncated u32 immediate");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_i32_immediate() {
        // Jump opcode followed by only 3 bytes.
        let err = decode(&[0x70, 0xFF, 0xFF, 0xFF]).unwrap_err();
        match err {
            BytecodeError::MalformedInstruction { reason, .. } => {
                assert_eq!(reason, "truncated i32 immediate");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn negative_jump_offset_round_trips() {
        let stream = vec![Instruction::Jump(-1_000_000)];
        let bytes = encode(&stream);
        let parsed = decode(&bytes).unwrap();
        assert_eq!(parsed, stream);
    }

    #[test]
    fn disassemble_text_format_is_stable() {
        let stream = vec![
            Instruction::LoadConst(0),
            Instruction::LoadConst(1),
            Instruction::Add,
            Instruction::Return,
        ];
        let text = disassemble_text(&stream);
        let expected = "0000  load_const  0\n\
                        0005  load_const  1\n\
                        000a  add\n\
                        000b  return\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn call_round_trips_with_two_u32_immediates() {
        let stream = vec![
            Instruction::Call { fn_idx: 3, argc: 2 },
            Instruction::Return,
        ];
        let bytes = encode(&stream);
        // 0x80, fn_idx=3 LE, argc=2 LE, 0x81
        assert_eq!(bytes, vec![0x80, 3, 0, 0, 0, 2, 0, 0, 0, 0x81]);
        let parsed = decode(&bytes).unwrap();
        assert_eq!(parsed, stream);
    }

    #[test]
    fn call_rejects_truncated_argc_immediate() {
        // Call opcode + fn_idx (4 bytes) + only 2 of argc's 4 bytes.
        let err = decode(&[0x80, 1, 0, 0, 0, 0, 0]).unwrap_err();
        match err {
            BytecodeError::MalformedInstruction { reason, .. } => {
                assert_eq!(reason, "truncated u32 immediate");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn disassemble_text_renders_call() {
        let stream = vec![Instruction::Call { fn_idx: 5, argc: 3 }];
        let text = disassemble_text(&stream);
        assert_eq!(text, "0000  call  5, 3\n");
    }

    #[test]
    fn disassemble_text_renders_negative_offsets() {
        let stream = vec![Instruction::Jump(-5)];
        let text = disassemble_text(&stream);
        assert_eq!(text, "0000  jump  -5\n");
    }
}
