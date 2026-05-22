//! Function table section (tag `0x02 Functions`).
//!
//! Wire layout:
//!
//! ```text
//! u32 count
//! count * (
//!     u32 name_len + name_bytes (UTF-8)
//!     u32 locals_count
//!     u32 code_len  + code_bytes (opaque opcodes; defined by S5+/S6+)
//! )
//! ```
//!
//! Opcodes themselves are not defined by S4b — the `code` slice is opaque
//! bytes that the bytecode emitter (S5) and VM (S6-S9) will own. This
//! keeps the section schema stable while the instruction set evolves
//! additively.

#![forbid(unsafe_code)]

use crate::cursor::Cursor;
use crate::error::BytecodeError;

/// One function in the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub locals_count: u32,
    pub code: Vec<u8>,
}

/// Decoded `Functions` section payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunctionTable {
    pub entries: Vec<Function>,
}

impl FunctionTable {
    /// Builds an empty function table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialises the table into the `Section` payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let count = self.entries.len() as u32;
        out.extend_from_slice(&count.to_le_bytes());
        for f in &self.entries {
            let name_len = f.name.len() as u32;
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(f.name.as_bytes());
            out.extend_from_slice(&f.locals_count.to_le_bytes());
            let code_len = f.code.len() as u32;
            out.extend_from_slice(&code_len.to_le_bytes());
            out.extend_from_slice(&f.code);
        }
        out
    }

    /// Parses the `Functions` section payload.
    pub fn decode(input: &[u8]) -> Result<Self, BytecodeError> {
        let mut cursor = Cursor::new(input);
        let count = cursor
            .read_u32_le()
            .ok_or(BytecodeError::MalformedFunctions {
                offset: cursor.pos(),
                reason: "truncated count",
            })? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let name = read_utf8_string(&mut cursor, "function name")?;
            let locals_count = cursor
                .read_u32_le()
                .ok_or(BytecodeError::MalformedFunctions {
                    offset: cursor.pos(),
                    reason: "truncated locals_count",
                })?;
            let code_len_pos = cursor.pos();
            let code_len = cursor
                .read_u32_le()
                .ok_or(BytecodeError::MalformedFunctions {
                    offset: code_len_pos,
                    reason: "truncated code_len",
                })? as usize;
            let code_pos = cursor.pos();
            let code_bytes = cursor
                .read_bytes(code_len)
                .ok_or(BytecodeError::MalformedFunctions {
                    offset: code_pos,
                    reason: "truncated code bytes",
                })?;
            entries.push(Function {
                name,
                locals_count,
                code: code_bytes.to_vec(),
            });
        }
        if !cursor.is_empty() {
            return Err(BytecodeError::MalformedFunctions {
                offset: cursor.pos(),
                reason: "trailing bytes after declared count",
            });
        }
        Ok(Self { entries })
    }
}

fn read_utf8_string(cursor: &mut Cursor<'_>, what: &'static str) -> Result<String, BytecodeError> {
    let len_pos = cursor.pos();
    let len = cursor
        .read_u32_le()
        .ok_or(BytecodeError::MalformedFunctions {
            offset: len_pos,
            reason: match what {
                "function name" => "truncated function name length",
                _ => "truncated string length",
            },
        })? as usize;
    let bytes_pos = cursor.pos();
    let bytes = cursor
        .read_bytes(len)
        .ok_or(BytecodeError::MalformedFunctions {
            offset: bytes_pos,
            reason: match what {
                "function name" => "truncated function name bytes",
                _ => "truncated string bytes",
            },
        })?;
    let s = std::str::from_utf8(bytes).map_err(|_| BytecodeError::MalformedFunctions {
        offset: bytes_pos,
        reason: match what {
            "function name" => "invalid UTF-8 in function name",
            _ => "invalid UTF-8 in string",
        },
    })?;
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_roundtrips() {
        let table = FunctionTable::new();
        let bytes = table.encode();
        assert_eq!(bytes, vec![0, 0, 0, 0]);
        assert_eq!(FunctionTable::decode(&bytes).unwrap(), table);
    }

    #[test]
    fn single_function_roundtrips() {
        let table = FunctionTable {
            entries: vec![Function {
                name: "main".to_string(),
                locals_count: 3,
                code: vec![0x10, 0x20, 0x30],
            }],
        };
        let bytes = table.encode();
        let parsed = FunctionTable::decode(&bytes).unwrap();
        assert_eq!(parsed, table);
    }

    #[test]
    fn multiple_functions_preserve_order() {
        let table = FunctionTable {
            entries: vec![
                Function {
                    name: "first".to_string(),
                    locals_count: 0,
                    code: Vec::new(),
                },
                Function {
                    name: "second".to_string(),
                    locals_count: 2,
                    code: vec![1, 2],
                },
                Function {
                    name: "third".to_string(),
                    locals_count: 5,
                    code: vec![0xFF],
                },
            ],
        };
        let parsed = FunctionTable::decode(&table.encode()).unwrap();
        assert_eq!(parsed.entries.len(), 3);
        for (a, b) in parsed.entries.iter().zip(&table.entries) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn rejects_truncated_count() {
        let err = FunctionTable::decode(&[0, 0]).unwrap_err();
        assert!(matches!(err, BytecodeError::MalformedFunctions { .. }));
    }

    #[test]
    fn rejects_invalid_utf8_name() {
        // count=1, name_len=2, name=0xFF 0xFE (invalid UTF-8)
        let buf = [1, 0, 0, 0, 2, 0, 0, 0, 0xFF, 0xFE];
        let err = FunctionTable::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedFunctions { reason, .. } => {
                assert_eq!(reason, "invalid UTF-8 in function name");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_trailing_bytes() {
        let table = FunctionTable::new();
        let mut bytes = table.encode();
        bytes.push(0xAA);
        let err = FunctionTable::decode(&bytes).unwrap_err();
        match err {
            BytecodeError::MalformedFunctions { reason, .. } => {
                assert_eq!(reason, "trailing bytes after declared count");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
