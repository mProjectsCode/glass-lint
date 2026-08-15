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
