//! Body section framing.
//!
//! Each section is encoded as:
//!
//! ```text
//! +--------+----------------+-----------------+
//! | tag u8 | length u32 LE  | payload (bytes) |
//! +--------+----------------+-----------------+
//! ```
//!
//! Tag values are fixed in v0 (`0x01 consts`, `0x02 functions`,
//! `0x03 imports`, `0x04 debug`). Section payload encoding is content-
//! specific and may be expanded additively within v0; S4 reserves the
//! framing but leaves the payload bytes opaque to the loader so the
//! emitter (S5+) and downstream tooling (debug-section parser, etc.) can
//! evolve independently.

#![forbid(unsafe_code)]

use crate::error::BytecodeError;

/// Size of a section header (`tag(u8) + length(u32 LE)`).
pub const SECTION_HEADER_SIZE: usize = 5;

/// Section tag. The numeric value is the byte stored in the encoded
/// stream and MUST match the table in `docs/bytecode-v0.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionTag {
    Consts = 0x01,
    Functions = 0x02,
    Imports = 0x03,
    Debug = 0x04,
}

impl SectionTag {
    /// Maps a wire byte back to a [`SectionTag`], or returns `None` for
    /// unknown values (the loader emits
    /// [`crate::BytecodeError::MalformedSection`] in that case).
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Consts),
            0x02 => Some(Self::Functions),
            0x03 => Some(Self::Imports),
            0x04 => Some(Self::Debug),
            _ => None,
        }
    }

    /// Wire byte for this tag.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }
}

/// One body section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub tag: SectionTag,
    pub payload: Vec<u8>,
}

impl Section {
    /// Builds a section with the given tag and payload bytes.
    #[must_use]
    pub fn new(tag: SectionTag, payload: Vec<u8>) -> Self {
        Self { tag, payload }
    }

    /// Serialises this section (header + payload). The payload length
    /// must fit in a `u32`; that limit is enforced by the loader on
    /// parse, so callers that synthesise giant payloads will fail-closed
    /// on the round-trip.
    pub fn serialize_into(&self, out: &mut Vec<u8>) {
        out.push(self.tag.as_byte());
        let len = self.payload.len() as u32;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.payload);
    }
}

/// Parses every section in `body`, returning the list and verifying that
/// the body bytes are fully consumed by section framing.
pub fn parse_sections(body: &[u8]) -> Result<Vec<Section>, BytecodeError> {
    let mut sections = Vec::new();
    let mut cursor = 0usize;
    while cursor < body.len() {
        if body.len() - cursor < SECTION_HEADER_SIZE {
            return Err(BytecodeError::MalformedSection {
                offset: cursor,
                reason: "truncated section header",
            });
        }
        let tag_byte = body[cursor];
        let tag = SectionTag::from_byte(tag_byte).ok_or(BytecodeError::MalformedSection {
            offset: cursor,
            reason: "unknown section tag",
        })?;
        let len = u32::from_le_bytes([
            body[cursor + 1],
            body[cursor + 2],
            body[cursor + 3],
            body[cursor + 4],
        ]) as usize;
        let payload_start = cursor + SECTION_HEADER_SIZE;
        let payload_end =
            payload_start
                .checked_add(len)
                .ok_or(BytecodeError::MalformedSection {
                    offset: cursor,
                    reason: "section length overflow",
                })?;
        if payload_end > body.len() {
            return Err(BytecodeError::MalformedSection {
                offset: cursor,
                reason: "section payload exceeds body",
            });
        }
        sections.push(Section {
            tag,
            payload: body[payload_start..payload_end].to_vec(),
        });
        cursor = payload_end;
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_round_trip() {
        for byte in 1..=4u8 {
            let tag = SectionTag::from_byte(byte).unwrap();
            assert_eq!(tag.as_byte(), byte);
        }
        assert!(SectionTag::from_byte(0).is_none());
        assert!(SectionTag::from_byte(99).is_none());
    }

    #[test]
    fn empty_body_parses_to_empty_section_list() {
        let sections = parse_sections(&[]).unwrap();
        assert!(sections.is_empty());
    }

    #[test]
    fn single_section_roundtrips() {
        let section = Section::new(SectionTag::Consts, vec![10, 20, 30]);
        let mut buf = Vec::new();
        section.serialize_into(&mut buf);
        // 1 tag + 4 length + 3 payload = 8 bytes
        assert_eq!(buf.len(), 8);
        assert_eq!(buf[0], 0x01);
        assert_eq!(&buf[1..5], &3u32.to_le_bytes());
        assert_eq!(&buf[5..8], &[10, 20, 30]);
        let parsed = parse_sections(&buf).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], section);
    }

    #[test]
    fn multiple_sections_roundtrip_in_order() {
        let a = Section::new(SectionTag::Consts, vec![1]);
        let b = Section::new(SectionTag::Functions, vec![2, 3]);
        let c = Section::new(SectionTag::Imports, vec![]);
        let d = Section::new(SectionTag::Debug, vec![4]);
        let mut buf = Vec::new();
        a.serialize_into(&mut buf);
        b.serialize_into(&mut buf);
        c.serialize_into(&mut buf);
        d.serialize_into(&mut buf);
        let parsed = parse_sections(&buf).unwrap();
        assert_eq!(parsed, vec![a, b, c, d]);
    }

    #[test]
    fn rejects_unknown_tag() {
        let buf = vec![0x99, 0, 0, 0, 0];
        let err = parse_sections(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedSection { reason, .. } => {
                assert_eq!(reason, "unknown section tag");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_header() {
        let buf = vec![0x01, 0, 0]; // only 3 bytes, need 5
        let err = parse_sections(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedSection { reason, .. } => {
                assert_eq!(reason, "truncated section header");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_payload_overflow() {
        // Declare 100-byte payload but only provide header
        let mut buf = vec![0x01];
        buf.extend_from_slice(&100u32.to_le_bytes());
        let err = parse_sections(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedSection { reason, .. } => {
                assert_eq!(reason, "section payload exceeds body");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
