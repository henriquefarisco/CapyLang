//! Top-level [`Module`] = header + body sections.
//!
//! Serialisation order is deterministic: the body is the concatenation of
//! [`Section::serialize_into`] for each section in the order they appear
//! in [`Module::sections`]. Two modules with identical fields therefore
//! produce identical bytes. The header's `body_length` and `checksum`
//! are derived from the body on every serialise so callers do not
//! have to keep them in sync manually.
//!
//! Parsing is fail-closed: every malformed byte sequence produces a
//! typed [`BytecodeError`] before any partial structural data is
//! exposed.

#![forbid(unsafe_code)]

use crate::checksum::{compute_checksum, Checksum};
use crate::error::BytecodeError;
use crate::header::{Header, HEADER_SIZE};
use crate::section::{parse_sections, Section};

/// A complete bytecode container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub abi_version: u16,
    pub sections: Vec<Section>,
}

impl Module {
    /// Builds a module with the given ABI version and sections. The
    /// `bc_version` is implicit (v0 for now).
    #[must_use]
    pub fn new(abi_version: u16, sections: Vec<Section>) -> Self {
        Self {
            abi_version,
            sections,
        }
    }

    /// Returns the bytecode container version. Currently always `0`
    /// (v0). Kept as a method to mirror the on-the-wire field.
    #[must_use]
    pub const fn bc_version(&self) -> u16 {
        0
    }

    /// Serialises the module into a fresh `Vec<u8>`. The header's
    /// `body_length` and `checksum` are recomputed from the body so the
    /// output is always self-consistent.
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for section in &self.sections {
            section.serialize_into(&mut body);
        }
        let checksum: Checksum = compute_checksum(&body);
        let body_length = body.len() as u32;
        let header = Header::new(self.bc_version(), self.abi_version, body_length, checksum);
        let mut out = Vec::with_capacity(HEADER_SIZE + body.len());
        out.extend_from_slice(&header.serialize());
        out.extend_from_slice(&body);
        out
    }

    /// Parses a module from `bytes`. Validates magic, version, flags,
    /// body length and BLAKE3-128 checksum **before** parsing any
    /// section content.
    pub fn parse(bytes: &[u8]) -> Result<Self, BytecodeError> {
        let (header, body_start) = Header::parse(bytes)?;
        let body = bytes
            .get(body_start..)
            .ok_or(BytecodeError::TruncatedHeader {
                available: bytes.len(),
            })?;
        let declared = header.body_length as usize;
        if body.len() != declared {
            return Err(BytecodeError::BodyLengthMismatch {
                declared: header.body_length,
                actual: body.len(),
            });
        }
        let computed = compute_checksum(body);
        if computed != header.checksum {
            return Err(BytecodeError::ChecksumMismatch {
                declared: header.checksum,
                computed,
            });
        }
        let sections = parse_sections(body)?;
        Ok(Self {
            abi_version: header.abi_version,
            sections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::MAGIC;
    use crate::section::SectionTag;

    #[test]
    fn empty_module_roundtrips() {
        let m = Module::new(0, Vec::new());
        let bytes = m.serialize();
        assert_eq!(bytes.len(), HEADER_SIZE);
        let parsed = Module::parse(&bytes).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn module_with_sections_roundtrips() {
        let m = Module::new(
            7,
            vec![
                Section::new(SectionTag::Consts, vec![1, 2, 3, 4]),
                Section::new(SectionTag::Functions, vec![]),
                Section::new(SectionTag::Imports, vec![0xAA, 0xBB]),
                Section::new(SectionTag::Debug, vec![0xCC]),
            ],
        );
        let bytes = m.serialize();
        let parsed = Module::parse(&bytes).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn corrupted_body_byte_fails_checksum() {
        let m = Module::new(0, vec![Section::new(SectionTag::Consts, vec![1, 2, 3])]);
        let mut bytes = m.serialize();
        // Flip one byte in the section payload (which lives after the 32-byte
        // header + the 5-byte section header).
        bytes[HEADER_SIZE + 5] ^= 0xFF;
        let err = Module::parse(&bytes).unwrap_err();
        assert!(matches!(err, BytecodeError::ChecksumMismatch { .. }));
    }

    #[test]
    fn truncated_body_fails_length_check() {
        let m = Module::new(0, vec![Section::new(SectionTag::Consts, vec![1, 2, 3])]);
        let mut bytes = m.serialize();
        bytes.pop(); // drop the last byte
        let err = Module::parse(&bytes).unwrap_err();
        assert!(matches!(err, BytecodeError::BodyLengthMismatch { .. }));
    }

    #[test]
    fn magic_at_start() {
        let m = Module::new(0, Vec::new());
        let bytes = m.serialize();
        assert_eq!(&bytes[0..4], &MAGIC);
    }

    #[test]
    fn rejects_unknown_major_inside_module() {
        let m = Module::new(0, Vec::new());
        let mut bytes = m.serialize();
        bytes[4] = 99; // bc_version low byte
        let err = Module::parse(&bytes).unwrap_err();
        assert!(matches!(err, BytecodeError::UnsupportedBcVersion { .. }));
    }

    #[test]
    fn deterministic_serialisation() {
        let m = Module::new(0, vec![Section::new(SectionTag::Consts, vec![1, 2, 3])]);
        let a = m.serialize();
        let b = m.serialize();
        assert_eq!(a, b);
    }
}
