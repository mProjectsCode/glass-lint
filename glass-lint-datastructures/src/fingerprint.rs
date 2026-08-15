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
mod tests;
