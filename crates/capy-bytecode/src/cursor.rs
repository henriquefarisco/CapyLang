//! Minimal bounds-checked byte cursor.
//!
//! Used by the per-section payload decoders to avoid open-coded indexing.
//! Every read returns `None` when the request would run past the end of
//! the buffer; section decoders translate that into a typed
//! [`crate::BytecodeError`] with a stable code.

#![forbid(unsafe_code)]

pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub(crate) fn read_u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    pub(crate) fn read_u32_le(&mut self) -> Option<u32> {
        let end = self.pos.checked_add(4)?;
        let chunk = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    }

    pub(crate) fn read_i64_le(&mut self) -> Option<i64> {
        let end = self.pos.checked_add(8)?;
        let chunk = self.data.get(self.pos..end)?;
        self.pos = end;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(chunk);
        Some(i64::from_le_bytes(buf))
    }

    pub(crate) fn read_f64_le(&mut self) -> Option<f64> {
        let end = self.pos.checked_add(8)?;
        let chunk = self.data.get(self.pos..end)?;
        self.pos = end;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(chunk);
        Some(f64::from_le_bytes(buf))
    }

    pub(crate) fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let chunk = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(chunk)
    }
}
