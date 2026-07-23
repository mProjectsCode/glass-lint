//! Deterministic FNV-1a fingerprint writer for cache keys and stable hashing.
//!
//! All fingerprints use a fixed seed, so they are deterministic across process
//! invocations and machine boundaries (no random seeds).

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

/// Write bytes into a running FNV-1a hash state.
pub fn fnv_write(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(FNV_PRIME);
    }
}

/// Begin a new FNV-1a hash with the standard offset basis.
pub fn fnv_init() -> u64 {
    FNV_OFFSET
}
