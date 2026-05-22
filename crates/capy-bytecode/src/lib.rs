//! CapyLang bytecode v0 container (slice S4).
//!
//! Implements the **frozen header** documented in `docs/bytecode-v0.md`
//! and the body framing that hosts the four section tags (`0x01` consts,
//! `0x02` functions, `0x03` imports, `0x04` debug).
//!
//! The loader is **deterministic** and **fail-closed**: every malformed
//! input produces a typed [`BytecodeError`] with a stable [`Code`](capy_bytecode_code)
//! string. Unknown majors are rejected. The header layout cannot drift
//! within v0 — adding a field is a breaking change and requires a major
//! version bump.
//!
//! # Example
//!
//! ```
//! use capy_bytecode::{Module, Section, SectionTag};
//!
//! let m = Module::new(0, vec![Section::new(SectionTag::Consts, vec![1, 2, 3])]);
//! let bytes = m.serialize();
//! let parsed = Module::parse(&bytes).unwrap();
//! assert_eq!(parsed.sections.len(), 1);
//! assert_eq!(parsed.sections[0].payload, vec![1, 2, 3]);
//! ```

#![forbid(unsafe_code)]

mod checksum;
mod consts;
mod cursor;
mod debug;
mod error;
mod functions;
mod header;
mod imports;
mod instruction;
mod module;
mod opcode;
mod section;
mod verify;

pub use checksum::{compute_checksum, Checksum, CHECKSUM_SIZE};
pub use consts::{Constant, ConstPool};
pub use debug::{DebugEntry, DebugInfo};
pub use error::{
    BytecodeError, B_BODY_LENGTH_MISMATCH, B_CHECKSUM_MISMATCH, B_MAGIC_MISMATCH,
    B_MALFORMED_CONSTANTS, B_MALFORMED_DEBUG, B_MALFORMED_FUNCTIONS, B_MALFORMED_IMPORTS,
    B_MALFORMED_INSTRUCTION, B_MALFORMED_SECTION, B_RESERVED_FLAGS_NONZERO, B_TRUNCATED_HEADER,
    B_UNSUPPORTED_BC_VERSION,
};
pub use functions::{Function, FunctionTable};
pub use header::{Header, HEADER_SIZE, MAGIC, MAX_SUPPORTED_BC_VERSION};
pub use imports::{Import, ImportTable};
pub use instruction::{decode, disassemble_text, encode, Instruction};
pub use module::Module;
pub use opcode::{Imm, Opcode};
pub use section::{Section, SectionTag, SECTION_HEADER_SIZE};
pub use verify::{
    verify_function, VerifyError, VerifyReport, B_VERIFIER_CALL_ARITY_OVERFLOW,
    B_VERIFIER_FALL_OFF_END, B_VERIFIER_INVALID_RETURN_DEPTH, B_VERIFIER_JUMP_OUT_OF_BOUNDS,
    B_VERIFIER_LOCAL_OUT_OF_BOUNDS, B_VERIFIER_STACK_INCONSISTENCY, B_VERIFIER_STACK_UNDERFLOW,
    B_VERIFIER_UNKNOWN_FUNCTION_INDEX,
};

// Re-exported for downstream code that wants to print the codes without
// pulling in `capy-diagnostics` directly.
pub mod capy_bytecode_code {
    //! Stable bytecode diagnostic codes (S4 + S4b + S5a). Use the
    //! constants from [`crate::error`] when constructing
    //! [`crate::BytecodeError`] values; these `&'static str` aliases
    //! mirror the planned `capy-diagnostics::Code` catalogue.
    pub use crate::error::{
        B_BODY_LENGTH_MISMATCH, B_CHECKSUM_MISMATCH, B_MAGIC_MISMATCH, B_MALFORMED_CONSTANTS,
        B_MALFORMED_DEBUG, B_MALFORMED_FUNCTIONS, B_MALFORMED_IMPORTS, B_MALFORMED_INSTRUCTION,
        B_MALFORMED_SECTION, B_RESERVED_FLAGS_NONZERO, B_TRUNCATED_HEADER,
        B_UNSUPPORTED_BC_VERSION,
    };
}
