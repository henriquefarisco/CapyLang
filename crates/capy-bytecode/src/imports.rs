//! Import table section (tag `0x03 Imports`).
//!
//! Wire layout:
//!
//! ```text
//! u32 count
//! count * (
//!     u32 module_len + module_bytes (UTF-8)
//!     u32 symbol_len + symbol_bytes (UTF-8)
//! )
//! ```
//!
//! Each entry declares a host ABI symbol the module imports (e.g.
//! `time::now`, `log::info`). The VM links these against the host adapter
//! at load time; v0 keeps the schema opaque so the host ABI shape can
//! evolve under `capy-lang-host` v0 additively.

#![forbid(unsafe_code)]

use crate::cursor::Cursor;
use crate::error::BytecodeError;

/// One imported host symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub symbol: String,
}

/// Decoded `Imports` section payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportTable {
    pub entries: Vec<Import>,
}

impl ImportTable {
    /// Builds an empty import table.
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
        for i in &self.entries {
            write_string(&mut out, &i.module);
            write_string(&mut out, &i.symbol);
        }
        out
    }

    /// Parses the `Imports` section payload.
    pub fn decode(input: &[u8]) -> Result<Self, BytecodeError> {
        let mut cursor = Cursor::new(input);
        let count = cursor
            .read_u32_le()
            .ok_or(BytecodeError::MalformedImports {
                offset: cursor.pos(),
                reason: "truncated count",
            })? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let module = read_string(&mut cursor, "module")?;
            let symbol = read_string(&mut cursor, "symbol")?;
            entries.push(Import { module, symbol });
        }
        if !cursor.is_empty() {
            return Err(BytecodeError::MalformedImports {
                offset: cursor.pos(),
                reason: "trailing bytes after declared count",
            });
        }
        Ok(Self { entries })
    }
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let len = s.len() as u32;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn read_string(cursor: &mut Cursor<'_>, what: &'static str) -> Result<String, BytecodeError> {
    let len_pos = cursor.pos();
    let len = cursor
        .read_u32_le()
        .ok_or(BytecodeError::MalformedImports {
            offset: len_pos,
            reason: match what {
                "module" => "truncated module length",
                "symbol" => "truncated symbol length",
                _ => "truncated string length",
            },
        })? as usize;
    let bytes_pos = cursor.pos();
    let bytes = cursor
        .read_bytes(len)
        .ok_or(BytecodeError::MalformedImports {
            offset: bytes_pos,
            reason: match what {
                "module" => "truncated module bytes",
                "symbol" => "truncated symbol bytes",
                _ => "truncated string bytes",
            },
        })?;
    let s = std::str::from_utf8(bytes).map_err(|_| BytecodeError::MalformedImports {
        offset: bytes_pos,
        reason: match what {
            "module" => "invalid UTF-8 in module name",
            "symbol" => "invalid UTF-8 in symbol name",
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
        let table = ImportTable::new();
        let parsed = ImportTable::decode(&table.encode()).unwrap();
        assert_eq!(parsed, table);
    }

    #[test]
    fn single_import_roundtrips() {
        let table = ImportTable {
            entries: vec![Import {
                module: "time".to_string(),
                symbol: "now".to_string(),
            }],
        };
        let parsed = ImportTable::decode(&table.encode()).unwrap();
        assert_eq!(parsed, table);
    }

    #[test]
    fn multiple_imports_preserve_order() {
        let table = ImportTable {
            entries: vec![
                Import {
                    module: "time".into(),
                    symbol: "now".into(),
                },
                Import {
                    module: "log".into(),
                    symbol: "info".into(),
                },
                Import {
                    module: "gfx2d".into(),
                    symbol: "clear".into(),
                },
            ],
        };
        let parsed = ImportTable::decode(&table.encode()).unwrap();
        assert_eq!(parsed, table);
    }

    #[test]
    fn rejects_invalid_utf8_module() {
        // count=1, module_len=2, module=0xFF 0xFE
        let buf = [1, 0, 0, 0, 2, 0, 0, 0, 0xFF, 0xFE];
        let err = ImportTable::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedImports { reason, .. } => {
                assert_eq!(reason, "invalid UTF-8 in module name");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_truncated_symbol() {
        // count=1, module_len=0, module="", symbol_len=10 (overflow)
        let buf = [1, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0];
        let err = ImportTable::decode(&buf).unwrap_err();
        match err {
            BytecodeError::MalformedImports { reason, .. } => {
                assert_eq!(reason, "truncated symbol bytes");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
