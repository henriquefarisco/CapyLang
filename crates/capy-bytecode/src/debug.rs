//! Debug info section (tag `0x04 Debug`).
//!
//! Wire layout:
//!
//! ```text
//! u32 count
//! count * (
//!     u32 bytecode_offset    (byte index inside the function code)
//!     u32 source_start       (byte offset in the original source)
//!     u32 source_end         (byte offset in the original source)
//! )
//! ```
//!
//! The section is **optional**: a module without debug spans simply omits
//! the section. Future S4 sub-slices may extend the per-entry layout with
//! additional fields appended after `source_end`; the schema is therefore
//! reserved at the entry boundary, not at the section boundary.

#![forbid(unsafe_code)]

use crate::cursor::Cursor;
use crate::error::BytecodeError;

/// One debug span linking a bytecode offset to a source byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugEntry {
    pub bytecode_offset: u32,
    pub source_start: u32,
    pub source_end: u32,
}

/// Decoded `Debug` section payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugInfo {
    pub entries: Vec<DebugEntry>,
}

impl DebugInfo {
    /// Builds an empty debug section.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialises the section into the `Section` payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.entries.len() * 12);
        let count = self.entries.len() as u32;
        out.extend_from_slice(&count.to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&e.bytecode_offset.to_le_bytes());
            out.extend_from_slice(&e.source_start.to_le_bytes());
            out.extend_from_slice(&e.source_end.to_le_bytes());
        }
        out
    }

    /// Parses the `Debug` section payload.
    pub fn decode(input: &[u8]) -> Result<Self, BytecodeError> {
        let mut cursor = Cursor::new(input);
        let count = cursor.read_u32_le().ok_or(BytecodeError::MalformedDebug {
            offset: cursor.pos(),
            reason: "truncated count",
        })? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let bytecode_offset = cursor.read_u32_le().ok_or(BytecodeError::MalformedDebug {
                offset: cursor.pos(),
                reason: "truncated bytecode_offset",
            })?;
            let source_start = cursor.read_u32_le().ok_or(BytecodeError::MalformedDebug {
                offset: cursor.pos(),
                reason: "truncated source_start",
            })?;
            let source_end_pos = cursor.pos();
            let source_end = cursor.read_u32_le().ok_or(BytecodeError::MalformedDebug {
                offset: source_end_pos,
                reason: "truncated source_end",
            })?;
            if source_end < source_start {
                return Err(BytecodeError::MalformedDebug {
                    offset: source_end_pos,
                    reason: "source_end precedes source_start",
                });
            }
            entries.push(DebugEntry {
                bytecode_offset,
                source_start,
                source_end,
            });
        }
        if !cursor.is_empty() {
            return Err(BytecodeError::MalformedDebug {
                offset: cursor.pos(),
                reason: "trailing bytes after declared count",
            });
        }
        Ok(Self { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_section_roundtrips() {
        let info = DebugInfo::new();
        let parsed = DebugInfo::decode(&info.encode()).unwrap();
        assert_eq!(parsed, info);
    }

    #[test]
    fn entries_preserve_order() {
        let info = DebugInfo {
            entries: vec![
                DebugEntry {
                    bytecode_offset: 0,
                    source_start: 0,
                    source_end: 5,
                },
                DebugEntry {
                    bytecode_offset: 8,
                    source_start: 10,
                    source_end: 12,
                },
            ],
        };
        let parsed = DebugInfo::decode(&info.encode()).unwrap();
        assert_eq!(parsed, info);
    }

    #[test]
    fn rejects_inverted_span() {
        // count=1, bytecode_offset=0, source_start=10, source_end=5
        let buf = [
            1, 0, 0, 0, // count
            0, 0, 0, 0, // bytecode_offset
            10, 0, 0, 0, // source_start
            5, 0, 0, 0, // source_end (< source_start)
        ];
        let err = DebugInfo::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedDebug { reason, .. } => {
                assert_eq!(reason, "source_end precedes source_start");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut buf = DebugInfo::new().encode();
        buf.push(0xCC);
        let err = DebugInfo::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedDebug { reason, .. } => {
                assert_eq!(reason, "trailing bytes after declared count");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
