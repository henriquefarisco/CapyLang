//! BLAKE3-128 checksum used by the bytecode container.
//!
//! BLAKE3-128 = first 16 bytes of the standard BLAKE3 output. Endian
//! ordering of the 16 bytes is identical to the BLAKE3 reference
//! implementation; the header field stores those bytes verbatim.

#![forbid(unsafe_code)]

/// Number of bytes in a [`Checksum`].
pub const CHECKSUM_SIZE: usize = 16;

/// Truncated BLAKE3-128 checksum, stored verbatim in the bytecode header.
pub type Checksum = [u8; CHECKSUM_SIZE];

/// Computes the BLAKE3-128 checksum of `bytes`.
#[must_use]
pub fn compute_checksum(bytes: &[u8]) -> Checksum {
    let full = blake3::hash(bytes);
    let full_bytes = *full.as_bytes();
    let mut out = [0u8; CHECKSUM_SIZE];
    out.copy_from_slice(&full_bytes[..CHECKSUM_SIZE]);
    out
}

#[cfg(test)]
mod tests {
    use super::compute_checksum;

    /// BLAKE3 of the empty input is the published reference value
    /// `af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262`;
    /// the first 16 bytes are BLAKE3-128.
    #[test]
    fn empty_input_matches_reference() {
        let got = compute_checksum(b"");
        assert_eq!(hex(&got), "af1349b9f5f9a1a6a0404dea36dcc949");
    }

    #[test]
    fn changing_one_byte_changes_the_digest() {
        let a = compute_checksum(b"hello");
        let b = compute_checksum(b"hellp");
        assert_ne!(a, b);
    }

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let hi = b >> 4;
            let lo = b & 0x0F;
            s.push(nibble(hi));
            s.push(nibble(lo));
        }
        s
    }

    fn nibble(n: u8) -> char {
        match n {
            0..=9 => (b'0' + n) as char,
            10..=15 => (b'a' + (n - 10)) as char,
            _ => unreachable!(),
        }
    }
}
