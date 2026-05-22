//! Frozen 32-byte v0 header.
//!
//! Layout (little-endian for every multi-byte integer):
//!
//! | Offset | Size | Field         |
//! |-------:|-----:|---------------|
//! |      0 |    4 | `magic`       |
//! |      4 |    2 | `bc_version`  |
//! |      6 |    2 | `abi_version` |
//! |      8 |    4 | `flags`       |
//! |     12 |    4 | `body_length` |
//! |     16 |   16 | `checksum`    |
//!
//! See `docs/bytecode-v0.md` for the authoritative specification.

#![forbid(unsafe_code)]

use crate::checksum::{Checksum, CHECKSUM_SIZE};
use crate::error::BytecodeError;

/// Total size of the header in bytes (frozen).
pub const HEADER_SIZE: usize = 32;

/// ASCII bytes that must appear at offset 0.
pub const MAGIC: [u8; 4] = *b"CAPY";

/// Highest `bc_version` accepted by this loader.
///
/// Per the contract in `docs/bytecode-v0.md`, the loader must accept the
/// current major plus the immediately preceding major. At v0 there is no
/// preceding major, so only `0` is accepted.
pub const MAX_SUPPORTED_BC_VERSION: u16 = 0;

/// Parsed v0 header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub bc_version: u16,
    pub abi_version: u16,
    pub flags: u32,
    pub body_length: u32,
    pub checksum: Checksum,
}

impl Header {
    /// Builds a header for the given parameters with `flags = 0` (the
    /// only legal value in v0).
    #[must_use]
    pub const fn new(
        bc_version: u16,
        abi_version: u16,
        body_length: u32,
        checksum: Checksum,
    ) -> Self {
        Self {
            bc_version,
            abi_version,
            flags: 0,
            body_length,
            checksum,
        }
    }

    /// Serialises the header into exactly [`HEADER_SIZE`] bytes.
    #[must_use]
    pub fn serialize(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..4].copy_from_slice(&MAGIC);
        out[4..6].copy_from_slice(&self.bc_version.to_le_bytes());
        out[6..8].copy_from_slice(&self.abi_version.to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.body_length.to_le_bytes());
        out[16..32].copy_from_slice(&self.checksum);
        out
    }

    /// Parses a header from the first [`HEADER_SIZE`] bytes of `input`.
    ///
    /// Validates magic, rejects unknown majors and rejects non-zero
    /// `flags`. Returns `(header, body_offset)`; `body_offset` is always
    /// [`HEADER_SIZE`] on success but is returned for parity with future
    /// versions that prepend optional sections.
    pub fn parse(input: &[u8]) -> Result<(Self, usize), BytecodeError> {
        if input.len() < HEADER_SIZE {
            return Err(BytecodeError::TruncatedHeader {
                available: input.len(),
            });
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&input[0..4]);
        if magic != MAGIC {
            return Err(BytecodeError::MagicMismatch { found: magic });
        }
        let bc_version = u16_le(&input[4..6]);
        if bc_version > MAX_SUPPORTED_BC_VERSION {
            return Err(BytecodeError::UnsupportedBcVersion {
                found: bc_version,
                max_supported: MAX_SUPPORTED_BC_VERSION,
            });
        }
        let abi_version = u16_le(&input[6..8]);
        let flags = u32_le(&input[8..12]);
        if flags != 0 {
            return Err(BytecodeError::ReservedFlagsNonZero { found: flags });
        }
        let body_length = u32_le(&input[12..16]);
        let mut checksum: Checksum = [0u8; CHECKSUM_SIZE];
        checksum.copy_from_slice(&input[16..32]);
        Ok((
            Self {
                bc_version,
                abi_version,
                flags,
                body_length,
                checksum,
            },
            HEADER_SIZE,
        ))
    }
}

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_then_parse_roundtrips() {
        let header = Header::new(0, 7, 42, [0xAA; CHECKSUM_SIZE]);
        let bytes = header.serialize();
        assert_eq!(bytes.len(), HEADER_SIZE);
        assert_eq!(&bytes[0..4], &MAGIC);
        let (parsed, off) = Header::parse(&bytes).unwrap();
        assert_eq!(off, HEADER_SIZE);
        assert_eq!(parsed, header);
    }

    #[test]
    fn rejects_short_input() {
        let err = Header::parse(&[0u8; 10]).unwrap_err();
        assert!(matches!(err, BytecodeError::TruncatedHeader { available: 10 }));
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = Header::new(0, 0, 0, [0; CHECKSUM_SIZE]).serialize();
        bytes[0] = b'X';
        let err = Header::parse(&bytes).unwrap_err();
        assert!(matches!(err, BytecodeError::MagicMismatch { .. }));
    }

    #[test]
    fn rejects_unknown_major() {
        let mut bytes = Header::new(0, 0, 0, [0; CHECKSUM_SIZE]).serialize();
        // bc_version is at offset 4..6, little-endian.
        bytes[4] = 99;
        bytes[5] = 0;
        let err = Header::parse(&bytes).unwrap_err();
        assert!(matches!(
            err,
            BytecodeError::UnsupportedBcVersion {
                found: 99,
                max_supported: 0
            }
        ));
    }

    #[test]
    fn rejects_nonzero_flags() {
        let mut bytes = Header::new(0, 0, 0, [0; CHECKSUM_SIZE]).serialize();
        // flags is at offset 8..12.
        bytes[8] = 1;
        let err = Header::parse(&bytes).unwrap_err();
        assert!(matches!(err, BytecodeError::ReservedFlagsNonZero { found: 1 }));
    }

    #[test]
    fn header_layout_is_32_bytes() {
        assert_eq!(HEADER_SIZE, 32);
    }
}
