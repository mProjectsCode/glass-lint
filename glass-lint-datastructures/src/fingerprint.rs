use std::fmt;

use xxhash_rust::xxh3::Xxh3;

/// A 64-bit XXH3 fingerprint (hash).
///
/// Deterministic, fast, non-cryptographic. Useful for content fingerprints,
/// stability fingerprints, and deduplication keys where collision resistance
/// beyond 64 bits is not required.
#[derive(Clone)]
pub struct Fingerprint(Xxh3);

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fingerprint")
            .field("hash", &self.clone().into_raw())
            .finish()
    }
}

impl PartialEq for Fingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.clone().into_raw() == other.clone().into_raw()
    }
}

impl Eq for Fingerprint {}

impl Default for Fingerprint {
    fn default() -> Self {
        Self::init()
    }
}

impl Fingerprint {
    /// Creates a fingerprint initialised to the XXH3 default state.
    pub fn init() -> Self {
        Self(Xxh3::new())
    }

    /// Absorbs `bytes` into the running fingerprint.
    pub fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Consumes the fingerprint and returns the raw 64-bit hash.
    pub fn into_raw(self) -> u64 {
        self.0.digest()
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
        assert_eq!(fp.into_raw(), Fingerprint::init().into_raw());
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
    fn empty_input_returns_init_state() {
        assert_eq!(hash_bytes(b""), Fingerprint::init().into_raw());
    }

    #[test]
    fn clone_produces_independent_fingerprints() {
        let mut a = Fingerprint::init();
        a.write(b"data");
        let mut b = a.clone();
        b.write(b"more");
        assert_ne!(a.into_raw(), b.into_raw());
    }

    #[test]
    fn clone_semantics() {
        let a = Fingerprint::init();
        let b = a.clone();
        assert_eq!(a, b);
    }
}
