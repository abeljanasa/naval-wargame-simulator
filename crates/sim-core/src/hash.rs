//! Stable state hash (ADR-0003 §5). FNV-1a 64 over an explicit, canonical byte encoding.
//! Floats are hashed by bit pattern, never by formatted text. This is a regression check, not a security hash.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a 64 hasher with explicit little-endian encodings.
#[derive(Debug, Clone)]
pub struct StateHash(u64);

impl Default for StateHash {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl StateHash {
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u128(&mut self, v: u128) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    pub fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }
    #[must_use]
    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_reference_vectors() {
        // Published FNV-1a 64 test vectors (Fowler/Noll/Vo).
        let h = StateHash::default();
        assert_eq!(h.finish(), 0xcbf2_9ce4_8422_2325);
        let mut h = StateHash::default();
        h.bytes(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
        let mut h = StateHash::default();
        h.bytes(b"foobar");
        assert_eq!(h.finish(), 0x8594_4171_f739_67e8);
    }
}
