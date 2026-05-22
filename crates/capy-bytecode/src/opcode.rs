//! v0 opcode catalogue.
//!
//! The opcode set is **frozen by S5a** within `capy-lang-host` v0 just
//! like the header layout: adding a new opcode is additive, renaming or
//! reusing a byte value is breaking. Every opcode value is recorded in
//! `docs/bytecode-v0.md` for cross-repo audit.
//!
//! Immediate encoding is fixed per opcode:
//!
//! * [`Imm::None`]  — opcode is one byte, no immediate.
//! * [`Imm::U32`]   — opcode is one byte followed by a 4-byte little-endian unsigned index.
//! * [`Imm::I32`]   — opcode is one byte followed by a 4-byte little-endian signed offset.
//!
//! For `Jump` / `JumpIfFalse`, the signed offset is relative to the byte
//! that follows the 4-byte immediate (PC after decoding the full
//! instruction). Offset `0` means "fall through to the next instruction".

#![forbid(unsafe_code)]

/// Shape of the immediate operand of an opcode, in bytes after the
/// opcode byte itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imm {
    /// No immediate; opcode is a single byte.
    None,
    /// 4-byte little-endian unsigned (used for indices).
    U32,
    /// 4-byte little-endian signed (used for relative jump offsets).
    I32,
    /// Two consecutive 4-byte little-endian unsigned integers. Used
    /// today only by `Call` to carry `(fn_idx, argc)`.
    U32U32,
}

impl Imm {
    /// Total instruction width in bytes (opcode + immediate).
    #[must_use]
    pub const fn width(self) -> usize {
        match self {
            Self::None => 1,
            Self::U32 | Self::I32 => 1 + 4,
            Self::U32U32 => 1 + 8,
        }
    }
}

/// Every legal opcode in v0. The numeric value is the byte stored in the
/// `Function.code` stream and **must** match the table in
/// `docs/bytecode-v0.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    /// Do nothing. Useful for alignment in tests and for padding.
    Nop = 0x00,
    /// Pop the top of the stack and discard it.
    Pop = 0x01,

    // === Constants ==========================================================
    /// `LoadConst u32` — push the constant pool entry at the given index.
    LoadConst = 0x10,
    /// Push the boolean `true`.
    LoadTrue = 0x11,
    /// Push the boolean `false`.
    LoadFalse = 0x12,
    /// Push the unit/`None` value.
    LoadNone = 0x13,

    // === Locals =============================================================
    /// `LoadLocal u32` — push the local at the given index.
    LoadLocal = 0x20,
    /// `StoreLocal u32` — pop the top of the stack into the local at the
    /// given index.
    StoreLocal = 0x21,

    // === Arithmetic =========================================================
    /// `a b -> a+b`
    Add = 0x30,
    /// `a b -> a-b`
    Sub = 0x31,
    /// `a b -> a*b`
    Mul = 0x32,
    /// `a b -> a/b`
    Div = 0x33,
    /// `a b -> a%b`
    Mod = 0x34,
    /// `a -> -a`
    Neg = 0x35,

    // === Comparison =========================================================
    /// `a b -> a==b`
    Eq = 0x40,
    /// `a b -> a!=b`
    Ne = 0x41,
    /// `a b -> a<b`
    Lt = 0x42,
    /// `a b -> a<=b`
    Le = 0x43,
    /// `a b -> a>b`
    Gt = 0x44,
    /// `a b -> a>=b`
    Ge = 0x45,

    // === Logical ============================================================
    /// `a -> !a`
    Not = 0x50,

    // === Control flow =======================================================
    /// `Jump i32` — unconditional relative jump.
    Jump = 0x70,
    /// `JumpIfFalse i32` — pop the top of the stack; if falsey, jump.
    JumpIfFalse = 0x71,

    // === Functions ==========================================================
    /// `Call (u32 fn_idx, u32 argc)` — pop `argc` arguments (left-to-right
    /// at emit time, top-of-stack is the last argument), push a new
    /// call frame and jump to the start of `functions[fn_idx]`. The
    /// callee's local slots `0..argc` are initialised with the popped
    /// arguments (slot `0` is the first argument); any extra local
    /// slots are initialised to `None`.
    Call = 0x80,
    /// Return from the current function. Top of stack (if any) is the
    /// return value; an empty stack implies unit. When the call stack
    /// is non-empty, control resumes at the instruction immediately
    /// after the matching `Call` site, with the return value pushed
    /// onto the caller's operand stack.
    Return = 0x81,

    // === Host bridge ========================================================
    /// `HostCall (u32 import_idx, u32 argc)` — pop `argc` arguments from
    /// the operand stack (left-to-right at emit time, top-of-stack is
    /// the last argument), look up `imports[import_idx]` in the
    /// module's `Imports` section, dispatch through the host adapter
    /// registered with the VM and push the returned [`crate`-level
    /// `Value`] back onto the operand stack. The host call never
    /// observes raw VM internals; the adapter receives a borrowed slice
    /// of `Value`s and must return a single `Value`. Stack-effect:
    /// pops `argc`, pushes 1 (net `1 - argc`). See `S7` in `README.md`
    /// and `docs/integration.md` for the contract.
    HostCall = 0x82,
}

impl Opcode {
    /// Maps a wire byte back to an [`Opcode`], or `None` for unknown
    /// values. Loaders translate `None` into
    /// [`crate::BytecodeError::MalformedInstruction`].
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0x00 => Self::Nop,
            0x01 => Self::Pop,
            0x10 => Self::LoadConst,
            0x11 => Self::LoadTrue,
            0x12 => Self::LoadFalse,
            0x13 => Self::LoadNone,
            0x20 => Self::LoadLocal,
            0x21 => Self::StoreLocal,
            0x30 => Self::Add,
            0x31 => Self::Sub,
            0x32 => Self::Mul,
            0x33 => Self::Div,
            0x34 => Self::Mod,
            0x35 => Self::Neg,
            0x40 => Self::Eq,
            0x41 => Self::Ne,
            0x42 => Self::Lt,
            0x43 => Self::Le,
            0x44 => Self::Gt,
            0x45 => Self::Ge,
            0x50 => Self::Not,
            0x70 => Self::Jump,
            0x71 => Self::JumpIfFalse,
            0x80 => Self::Call,
            0x81 => Self::Return,
            0x82 => Self::HostCall,
            _ => return None,
        })
    }

    /// Wire byte for this opcode.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// Shape of the immediate operand.
    #[must_use]
    pub const fn immediate(self) -> Imm {
        match self {
            Self::LoadConst | Self::LoadLocal | Self::StoreLocal => Imm::U32,
            Self::Jump | Self::JumpIfFalse => Imm::I32,
            Self::Call | Self::HostCall => Imm::U32U32,
            _ => Imm::None,
        }
    }

    /// Human-readable mnemonic. Stable across minor versions and used by
    /// [`crate::instruction::disassemble_text`].
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Nop => "nop",
            Self::Pop => "pop",
            Self::LoadConst => "load_const",
            Self::LoadTrue => "load_true",
            Self::LoadFalse => "load_false",
            Self::LoadNone => "load_none",
            Self::LoadLocal => "load_local",
            Self::StoreLocal => "store_local",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Mod => "mod",
            Self::Neg => "neg",
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Not => "not",
            Self::Jump => "jump",
            Self::JumpIfFalse => "jump_if_false",
            Self::Call => "call",
            Self::Return => "return",
            Self::HostCall => "host_call",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_byte_round_trip_for_all_variants() {
        let all = [
            Opcode::Nop,
            Opcode::Pop,
            Opcode::LoadConst,
            Opcode::LoadTrue,
            Opcode::LoadFalse,
            Opcode::LoadNone,
            Opcode::LoadLocal,
            Opcode::StoreLocal,
            Opcode::Add,
            Opcode::Sub,
            Opcode::Mul,
            Opcode::Div,
            Opcode::Mod,
            Opcode::Neg,
            Opcode::Eq,
            Opcode::Ne,
            Opcode::Lt,
            Opcode::Le,
            Opcode::Gt,
            Opcode::Ge,
            Opcode::Not,
            Opcode::Jump,
            Opcode::JumpIfFalse,
            Opcode::Call,
            Opcode::Return,
            Opcode::HostCall,
        ];
        for op in all {
            let b = op.as_byte();
            assert_eq!(Opcode::from_byte(b), Some(op), "mnemonic={}", op.mnemonic());
        }
    }

    #[test]
    fn unknown_bytes_return_none() {
        // 0x02..0x0F is reserved space inside the "stack manipulation" block.
        assert!(Opcode::from_byte(0x02).is_none());
        assert!(Opcode::from_byte(0x99).is_none());
        assert!(Opcode::from_byte(0xFF).is_none());
    }

    #[test]
    fn immediate_widths_match_expectations() {
        assert_eq!(Opcode::Nop.immediate(), Imm::None);
        assert_eq!(Opcode::Nop.immediate().width(), 1);
        assert_eq!(Opcode::LoadConst.immediate(), Imm::U32);
        assert_eq!(Opcode::LoadConst.immediate().width(), 5);
        assert_eq!(Opcode::Jump.immediate(), Imm::I32);
        assert_eq!(Opcode::Jump.immediate().width(), 5);
        assert_eq!(Opcode::Call.immediate(), Imm::U32U32);
        assert_eq!(Opcode::Call.immediate().width(), 9);
    }

    #[test]
    fn opcode_byte_values_are_unique() {
        let bytes: Vec<u8> = [
            Opcode::Nop,
            Opcode::Pop,
            Opcode::LoadConst,
            Opcode::LoadTrue,
            Opcode::LoadFalse,
            Opcode::LoadNone,
            Opcode::LoadLocal,
            Opcode::StoreLocal,
            Opcode::Add,
            Opcode::Sub,
            Opcode::Mul,
            Opcode::Div,
            Opcode::Mod,
            Opcode::Neg,
            Opcode::Eq,
            Opcode::Ne,
            Opcode::Lt,
            Opcode::Le,
            Opcode::Gt,
            Opcode::Ge,
            Opcode::Not,
            Opcode::Jump,
            Opcode::JumpIfFalse,
            Opcode::Call,
            Opcode::Return,
            Opcode::HostCall,
        ]
        .iter()
        .map(|o| o.as_byte())
        .collect();
        let mut sorted = bytes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            bytes.len(),
            sorted.len(),
            "opcode byte values must be unique"
        );
    }
}
