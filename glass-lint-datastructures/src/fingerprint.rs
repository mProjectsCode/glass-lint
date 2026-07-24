/// FNV-1a offset basis (64-bit).
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a prime (64-bit).
const FNV_PRIME: u64 = 0x100_0000_01b3;

/// Absorbs `bytes` into the running FNV-1a hash held in `h`.
pub(crate) fn fnv_write(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(FNV_PRIME);
    }
}

/// Returns the FNV-1a offset basis.
#[allow(dead_code)]
pub(crate) fn fnv_init() -> u64 {
    FNV_OFFSET
}

/// A 64-bit FNV-1a fingerprint (hash).
///
/// Deterministic, fast, non-cryptographic. Useful for content fingerprints,
/// stability fingerprints, and deduplication keys where collision resistance
/// beyond 64 bits is not required.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fingerprint(u64);

impl Default for Fingerprint {
    fn default() -> Self {
        Self::init()
    }
}

impl Fingerprint {
    /// Creates a fingerprint initialised to the FNV-1a offset basis.
    pub fn init() -> Self {
        Self(FNV_OFFSET)
    }

    /// Absorbs `bytes` into the running fingerprint.
    pub fn write(&mut self, bytes: &[u8]) {
        fnv_write(&mut self.0, bytes);
    }

    /// Consumes the fingerprint and returns the raw 64-bit hash.
    pub fn into_raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut fp = Fingerprint::init();
        fp.write(bytes);
        fp.into_raw()
    }

    #[test]
    fn deterministic_output_for_same_input() {
        let mut a = Fingerprint::init();
        let mut b = Fingerprint::init();
        a.write(b"hello");
        b.write(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn different_inputs_produce_different_hashes() {
        let mut a = Fingerprint::init();
        let mut b = Fingerprint::init();
        a.write(b"hello");
        b.write(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn into_raw_returns_the_raw_u64() {
        let fp = Fingerprint::init();
        assert_eq!(fp.into_raw(), FNV_OFFSET);
    }

    #[test]
    fn default_is_same_as_init() {
        assert_eq!(Fingerprint::default(), Fingerprint::init());
    }

    #[test]
    fn incremental_write_accumulates() {
        let mut fp = Fingerprint::init();
        fp.write(b"a");
        fp.write(b"b");
        let combined = hash_bytes(b"ab");
        assert_eq!(fp.into_raw(), combined);
    }

    #[test]
    fn empty_write_is_noop() {
        let fp = Fingerprint::init();
        let after = {
            let mut f = Fingerprint::init();
            f.write(b"");
            f.into_raw()
        };
        assert_eq!(fp.into_raw(), after);
    }

    #[test]
    fn empty_input_returns_offset_basis() {
        assert_eq!(hash_bytes(b""), FNV_OFFSET);
    }

    #[test]
    fn clone_produces_independent_fingerprints() {
        let mut a = Fingerprint::init();
        a.write(b"data");
        let mut b = a;
        b.write(b"more");
        assert_ne!(a.into_raw(), b.into_raw());
    }

    #[test]
    fn copy_semantics() {
        let a = Fingerprint::init();
        let b = a;
        assert_eq!(a, b);
    }
}
