//! Bytecode loader errors and stable diagnostic codes.
//!
//! Codes are part of the `capy-lang-host` v0 ABI. The catalogue is
//! additive within v0; renaming or removing a code is a breaking change.

#![forbid(unsafe_code)]

use std::fmt;

/// Header magic bytes did not match ASCII `CAPY`.
pub const B_MAGIC_MISMATCH: &str = "B0001";
/// `bc_version` in the header exceeds the loader's maximum supported
/// version.
pub const B_UNSUPPORTED_BC_VERSION: &str = "B0002";
/// The reserved `flags` field is non-zero in a v0 container.
pub const B_RESERVED_FLAGS_NONZERO: &str = "B0003";
/// The body bytes available do not match `body_length` declared in the
/// header.
pub const B_BODY_LENGTH_MISMATCH: &str = "B0004";
/// The BLAKE3-128 checksum recorded in the header does not match the
/// body bytes.
pub const B_CHECKSUM_MISMATCH: &str = "B0005";
/// A section tag is unknown, or a section length exceeds the remaining
/// body bytes, or the section header itself is truncated.
pub const B_MALFORMED_SECTION: &str = "B0006";
/// The input is shorter than the 32-byte header.
pub const B_TRUNCATED_HEADER: &str = "B0007";
/// Constant-pool section payload is malformed (truncated, unknown const
/// tag, invalid UTF-8 in a `Str`, or trailing bytes).
pub const B_MALFORMED_CONSTANTS: &str = "B0008";
/// Function-table section payload is malformed.
pub const B_MALFORMED_FUNCTIONS: &str = "B0009";
/// Import-table section payload is malformed.
pub const B_MALFORMED_IMPORTS: &str = "B0010";
/// Debug-info section payload is malformed.
pub const B_MALFORMED_DEBUG: &str = "B0011";
/// A bytecode instruction stream is malformed (unknown opcode, truncated
/// immediate or trailing garbage inside a `Function.code` slice).
pub const B_MALFORMED_INSTRUCTION: &str = "B0012";

/// Loader and serializer error variants. Every variant carries a stable
/// `&'static str` code in [`BytecodeError::code`] so downstream tooling
/// (renderer, CapyOS adapter) can branch deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BytecodeError {
    /// Header magic bytes did not match `CAPY`.
    MagicMismatch { found: [u8; 4] },
    /// `bc_version` is newer than this loader supports.
    UnsupportedBcVersion { found: u16, max_supported: u16 },
    /// Reserved `flags` field was non-zero in a v0 container.
    ReservedFlagsNonZero { found: u32 },
    /// Body bytes available do not match the declared `body_length`.
    BodyLengthMismatch { declared: u32, actual: usize },
    /// BLAKE3-128 checksum did not match the body bytes.
    ChecksumMismatch {
        declared: [u8; 16],
        computed: [u8; 16],
    },
    /// A section had an unknown tag, a truncated header or a payload
    /// length that overflowed the body.
    MalformedSection { offset: usize, reason: &'static str },
    /// The input did not contain the full 32-byte header.
    TruncatedHeader { available: usize },
    /// Constant-pool section payload is malformed.
    MalformedConstants { offset: usize, reason: &'static str },
    /// Function-table section payload is malformed.
    MalformedFunctions { offset: usize, reason: &'static str },
    /// Import-table section payload is malformed.
    MalformedImports { offset: usize, reason: &'static str },
    /// Debug-info section payload is malformed.
    MalformedDebug { offset: usize, reason: &'static str },
    /// Instruction stream inside a function is malformed (unknown opcode
    /// or truncated immediate).
    MalformedInstruction { offset: usize, reason: &'static str },
}

impl BytecodeError {
    /// Stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MagicMismatch { .. } => B_MAGIC_MISMATCH,
            Self::UnsupportedBcVersion { .. } => B_UNSUPPORTED_BC_VERSION,
            Self::ReservedFlagsNonZero { .. } => B_RESERVED_FLAGS_NONZERO,
            Self::BodyLengthMismatch { .. } => B_BODY_LENGTH_MISMATCH,
            Self::ChecksumMismatch { .. } => B_CHECKSUM_MISMATCH,
            Self::MalformedSection { .. } => B_MALFORMED_SECTION,
            Self::TruncatedHeader { .. } => B_TRUNCATED_HEADER,
            Self::MalformedConstants { .. } => B_MALFORMED_CONSTANTS,
            Self::MalformedFunctions { .. } => B_MALFORMED_FUNCTIONS,
            Self::MalformedImports { .. } => B_MALFORMED_IMPORTS,
            Self::MalformedDebug { .. } => B_MALFORMED_DEBUG,
            Self::MalformedInstruction { .. } => B_MALFORMED_INSTRUCTION,
        }
    }
}

/// Human-readable rendering for [`BytecodeError`].
///
/// Format is deterministic and includes the stable `B<NNNN>` code in
/// square brackets so downstream tooling (the `capyc` CLI, the
/// `capy-diagnostics` bridge and the eventual CapyOS adapter) can
/// route on the code while still presenting a readable line. Stable
/// within v0; snapshot tests should match the code, not the prose.
impl fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::MagicMismatch { found } => write!(
                f,
                "[{code}] header magic is not `CAPY` (found {found:02x?})"
            ),
            Self::UnsupportedBcVersion {
                found,
                max_supported,
            } => write!(
                f,
                "[{code}] unsupported bc_version {found} (max supported {max_supported})"
            ),
            Self::ReservedFlagsNonZero { found } => write!(
                f,
                "[{code}] reserved header flags must be zero (found 0x{found:08x})"
            ),
            Self::BodyLengthMismatch { declared, actual } => write!(
                f,
                "[{code}] header body_length={declared} does not match input ({actual} bytes available)"
            ),
            Self::ChecksumMismatch { declared, computed } => write!(
                f,
                "[{code}] BLAKE3-128 checksum mismatch (declared {declared:02x?}, computed {computed:02x?})"
            ),
            Self::MalformedSection { offset, reason } => write!(
                f,
                "[{code}] malformed section at body offset 0x{offset:04x}: {reason}"
            ),
            Self::TruncatedHeader { available } => write!(
                f,
                "[{code}] truncated header ({available} bytes available, 32 required)"
            ),
            Self::MalformedConstants { offset, reason } => write!(
                f,
                "[{code}] malformed constants section at payload offset 0x{offset:04x}: {reason}"
            ),
            Self::MalformedFunctions { offset, reason } => write!(
                f,
                "[{code}] malformed functions section at payload offset 0x{offset:04x}: {reason}"
            ),
            Self::MalformedImports { offset, reason } => write!(
                f,
                "[{code}] malformed imports section at payload offset 0x{offset:04x}: {reason}"
            ),
            Self::MalformedDebug { offset, reason } => write!(
                f,
                "[{code}] malformed debug section at payload offset 0x{offset:04x}: {reason}"
            ),
            Self::MalformedInstruction { offset, reason } => write!(
                f,
                "[{code}] malformed instruction at byte offset 0x{offset:04x}: {reason}"
            ),
        }
    }
}

impl std::error::Error for BytecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_stable_code_prefix() {
        let e = BytecodeError::MagicMismatch { found: *b"NOPE" };
        let s = format!("{e}");
        assert!(s.starts_with("[B0001]"), "got {s:?}");
    }

    #[test]
    fn display_renders_offset_in_hex() {
        let e = BytecodeError::MalformedInstruction {
            offset: 0xCAFE,
            reason: "unknown opcode",
        };
        assert_eq!(
            format!("{e}"),
            "[B0012] malformed instruction at byte offset 0xcafe: unknown opcode"
        );
    }

    #[test]
    fn every_variant_displays_with_its_code() {
        let samples: Vec<(BytecodeError, &'static str)> = vec![
            (
                BytecodeError::MagicMismatch { found: *b"XYZW" },
                B_MAGIC_MISMATCH,
            ),
            (
                BytecodeError::UnsupportedBcVersion {
                    found: 99,
                    max_supported: 0,
                },
                B_UNSUPPORTED_BC_VERSION,
            ),
            (
                BytecodeError::ReservedFlagsNonZero { found: 1 },
                B_RESERVED_FLAGS_NONZERO,
            ),
            (
                BytecodeError::BodyLengthMismatch {
                    declared: 10,
                    actual: 5,
                },
                B_BODY_LENGTH_MISMATCH,
            ),
            (
                BytecodeError::ChecksumMismatch {
                    declared: [0; 16],
                    computed: [1; 16],
                },
                B_CHECKSUM_MISMATCH,
            ),
            (
                BytecodeError::MalformedSection {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_SECTION,
            ),
            (
                BytecodeError::TruncatedHeader { available: 0 },
                B_TRUNCATED_HEADER,
            ),
            (
                BytecodeError::MalformedConstants {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_CONSTANTS,
            ),
            (
                BytecodeError::MalformedFunctions {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_FUNCTIONS,
            ),
            (
                BytecodeError::MalformedImports {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_IMPORTS,
            ),
            (
                BytecodeError::MalformedDebug {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_DEBUG,
            ),
            (
                BytecodeError::MalformedInstruction {
                    offset: 0,
                    reason: "r",
                },
                B_MALFORMED_INSTRUCTION,
            ),
        ];
        for (err, code) in samples {
            let s = format!("{err}");
            assert!(
                s.starts_with(&format!("[{code}]")),
                "variant {err:?} does not prefix with its code {code}: {s:?}"
            );
        }
    }
}
