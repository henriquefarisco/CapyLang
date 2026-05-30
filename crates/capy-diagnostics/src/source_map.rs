//! Byte offset → `(line, column)` translation.
//!
//! Lines are 1-indexed; columns are 1-indexed and counted in **bytes** from
//! the start of the line. Multi-byte UTF-8 sequences therefore contribute
//! their byte width to the column index. This matches the convention used
//! by most CLI tools (rustc, clang) and is the cheapest computation that
//! still keeps spans deterministic across platforms.
//!
//! Construction is `O(n)` over the source; queries are `O(log n)` via
//! binary search over the per-line offset table.

#![forbid(unsafe_code)]

/// Source text indexed by line.
#[derive(Debug, Clone)]
pub struct SourceMap<'a> {
    source: &'a str,
    /// Byte offset of the start of each line, in ascending order. Always
    /// contains at least `0`.
    line_starts: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    /// Builds a [`SourceMap`] over `source` in `O(n)`.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = Vec::with_capacity(8);
        line_starts.push(0);
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// Returns the underlying source.
    #[must_use]
    pub const fn source(&self) -> &'a str {
        self.source
    }

    /// Returns the total number of lines (always ≥ 1, even for an empty
    /// source).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Translates a byte offset into a `(line, column)` pair, both
    /// 1-indexed. Out-of-range offsets are clamped to `source.len()`.
    #[must_use]
    pub fn position(&self, byte_offset: usize) -> (usize, usize) {
        let offset = byte_offset.min(self.source.len());
        // `partition_point` returns the first index whose start is `> offset`.
        // The owning line is therefore `pp - 1`. `pp` is always ≥ 1 because
        // `line_starts[0] == 0 ≤ offset`.
        let pp = self.line_starts.partition_point(|&s| s <= offset);
        let line_idx = pp - 1;
        let line_start = self.line_starts[line_idx];
        let col_bytes = offset - line_start;
        (line_idx + 1, col_bytes + 1)
    }

    /// Returns the text of the given 1-indexed `line`, **without** the
    /// trailing `\n` (and `\r` for CRLF input). Returns `""` if `line` is
    /// out of range.
    #[must_use]
    pub fn line_text(&self, line: usize) -> &'a str {
        if line == 0 || line > self.line_starts.len() {
            return "";
        }
        let start = self.line_starts[line - 1];
        let end_excl = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.source.len());
        let mut text = &self.source[start..end_excl];
        while text.ends_with('\n') || text.ends_with('\r') {
            text = &text[..text.len() - 1];
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::SourceMap;

    #[test]
    fn empty_source() {
        let m = SourceMap::new("");
        assert_eq!(m.line_count(), 1);
        assert_eq!(m.position(0), (1, 1));
        assert_eq!(m.line_text(1), "");
    }

    #[test]
    fn single_line_no_newline() {
        let m = SourceMap::new("abc");
        assert_eq!(m.position(0), (1, 1));
        assert_eq!(m.position(1), (1, 2));
        assert_eq!(m.position(3), (1, 4)); // one past the end
        assert_eq!(m.line_text(1), "abc");
    }

    #[test]
    fn multi_line() {
        // "ab\ncd\nef"
        //  0  1  2  3  4  5  6  7
        let m = SourceMap::new("ab\ncd\nef");
        assert_eq!(m.position(0), (1, 1));
        assert_eq!(m.position(2), (1, 3)); // the '\n' itself
        assert_eq!(m.position(3), (2, 1));
        assert_eq!(m.position(4), (2, 2));
        assert_eq!(m.position(6), (3, 1));
        assert_eq!(m.line_text(1), "ab");
        assert_eq!(m.line_text(2), "cd");
        assert_eq!(m.line_text(3), "ef");
    }

    #[test]
    fn crlf_is_trimmed_from_line_text() {
        let m = SourceMap::new("ab\r\ncd\r\n");
        assert_eq!(m.line_text(1), "ab");
        assert_eq!(m.line_text(2), "cd");
    }

    #[test]
    fn out_of_range_line() {
        let m = SourceMap::new("abc");
        assert_eq!(m.line_text(0), "");
        assert_eq!(m.line_text(99), "");
    }
}
