//! Constant pool section (tag `0x01 Consts`).
//!
//! Wire layout:
//!
//! ```text
//! u32 count
//! count * (u8 const_tag + payload)
//! ```
//!
//! Constant tags (additive within v0):
//!
//! | Tag  | Variant       | Payload                                  |
//! |-----:|---------------|------------------------------------------|
//! | 0x01 | `Int(i64)`    | 8-byte little-endian signed integer      |
//! | 0x02 | `Float(f64)`  | 8-byte little-endian IEEE-754 binary64   |
//! | 0x03 | `Str(String)` | u32 byte length + UTF-8 bytes            |
//!
//! The decoder validates UTF-8 for strings and rejects unknown const tags.

#![forbid(unsafe_code)]

use crate::cursor::Cursor;
use crate::error::BytecodeError;

/// Constant pool entry tag bytes.
const CONST_TAG_INT: u8 = 0x01;
const CONST_TAG_FLOAT: u8 = 0x02;
const CONST_TAG_STR: u8 = 0x03;

/// A single entry in the constant pool.
///
/// `f64` makes the whole enum non-`Eq`; use `PartialEq` for comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Int(i64),
    Float(f64),
    Str(String),
}

/// Decoded `Consts` section payload.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConstPool {
    pub entries: Vec<Constant>,
}

impl ConstPool {
    /// Builds an empty pool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialises the pool into the `Section` payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.entries.len() * 9);
        let count = self.entries.len() as u32;
        out.extend_from_slice(&count.to_le_bytes());
        for c in &self.entries {
            match c {
                Constant::Int(v) => {
                    out.push(CONST_TAG_INT);
                    out.extend_from_slice(&v.to_le_bytes());
                }
                Constant::Float(v) => {
                    out.push(CONST_TAG_FLOAT);
                    out.extend_from_slice(&v.to_le_bytes());
                }
                Constant::Str(s) => {
                    out.push(CONST_TAG_STR);
                    let len = s.len() as u32;
                    out.extend_from_slice(&len.to_le_bytes());
                    out.extend_from_slice(s.as_bytes());
                }
            }
        }
        out
    }

    /// Parses the `Consts` section payload, returning a typed pool.
    ///
    /// Fail-closed: trailing bytes after the declared count, unknown const
    /// tags, length overflows and invalid UTF-8 are all rejected with
    /// [`BytecodeError::MalformedConstants`].
    pub fn decode(input: &[u8]) -> Result<Self, BytecodeError> {
        let mut cursor = Cursor::new(input);
        let count = cursor
            .read_u32_le()
            .ok_or(BytecodeError::MalformedConstants {
                offset: cursor.pos(),
                reason: "truncated count",
            })? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let tag_pos = cursor.pos();
            let tag = cursor.read_u8().ok_or(BytecodeError::MalformedConstants {
                offset: tag_pos,
                reason: "truncated entry tag",
            })?;
            match tag {
                CONST_TAG_INT => {
                    let v = cursor
                        .read_i64_le()
                        .ok_or(BytecodeError::MalformedConstants {
                            offset: cursor.pos(),
                            reason: "truncated Int payload",
                        })?;
                    entries.push(Constant::Int(v));
                }
                CONST_TAG_FLOAT => {
                    let v = cursor
                        .read_f64_le()
                        .ok_or(BytecodeError::MalformedConstants {
                            offset: cursor.pos(),
                            reason: "truncated Float payload",
                        })?;
                    entries.push(Constant::Float(v));
                }
                CONST_TAG_STR => {
                    let len_pos = cursor.pos();
                    let len = cursor
                        .read_u32_le()
                        .ok_or(BytecodeError::MalformedConstants {
                            offset: len_pos,
                            reason: "truncated Str length",
                        })? as usize;
                    let bytes_pos = cursor.pos();
                    let bytes =
                        cursor
                            .read_bytes(len)
                            .ok_or(BytecodeError::MalformedConstants {
                                offset: bytes_pos,
                                reason: "truncated Str bytes",
                            })?;
                    let s = std::str::from_utf8(bytes).map_err(|_| {
                        BytecodeError::MalformedConstants {
                            offset: bytes_pos,
                            reason: "invalid UTF-8 in Str",
                        }
                    })?;
                    entries.push(Constant::Str(s.to_string()));
                }
                _ => {
                    return Err(BytecodeError::MalformedConstants {
                        offset: tag_pos,
                        reason: "unknown constant tag",
                    });
                }
            }
        }
        if !cursor.is_empty() {
            return Err(BytecodeError::MalformedConstants {
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
    fn empty_pool_roundtrips() {
        let pool = ConstPool::new();
        let bytes = pool.encode();
        assert_eq!(bytes, vec![0, 0, 0, 0]);
        assert_eq!(ConstPool::decode(&bytes).unwrap(), pool);
    }

    #[test]
    fn int_float_string_roundtrip() {
        let pool = ConstPool {
            entries: vec![
                Constant::Int(-42),
                Constant::Float(3.125),
                Constant::Str("hello, café".to_string()),
            ],
        };
        let bytes = pool.encode();
        let parsed = ConstPool::decode(&bytes).unwrap();
        assert_eq!(parsed, pool);
    }

    #[test]
    fn rejects_unknown_const_tag() {
        // count=1, tag=0xFF
        let buf = [1, 0, 0, 0, 0xFF];
        let err = ConstPool::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedConstants { reason, .. } => {
                assert_eq!(reason, "unknown constant tag");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_utf8_string() {
        // count=1, tag=0x03 Str, len=2, bytes=0xFF 0xFE (invalid UTF-8)
        let buf = [1, 0, 0, 0, 0x03, 2, 0, 0, 0, 0xFF, 0xFE];
        let err = ConstPool::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedConstants { reason, .. } => {
                assert_eq!(reason, "invalid UTF-8 in Str");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_payload() {
        // count=1, tag=0x01 Int, no payload
        let buf = [1, 0, 0, 0, 0x01];
        let err = ConstPool::decode(&buf).unwrap_err();
        assert!(matches!(err, BytecodeError::MalformedConstants { .. }));
    }

    #[test]
    fn rejects_trailing_bytes() {
        // count=0 then a spurious byte
        let buf = [0, 0, 0, 0, 0xAA];
        let err = ConstPool::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedConstants { reason, .. } => {
                assert_eq!(reason, "trailing bytes after declared count");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn negative_zero_float_roundtrips() {
        let pool = ConstPool {
            entries: vec![Constant::Float(-0.0)],
        };
        let parsed = ConstPool::decode(&pool.encode()).unwrap();
        // -0.0 and 0.0 compare equal under PartialEq for f64, but the bit
        // pattern is distinct. Verify we preserve the bits exactly.
        if let Constant::Float(v) = parsed.entries[0] {
            assert_eq!(v.to_bits(), (-0.0f64).to_bits());
        } else {
            panic!("expected Float");
        }
    }
}
