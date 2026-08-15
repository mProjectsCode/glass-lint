use super::*;

#[test]
fn repeated_names_share_ids_and_invalid_ids_fail_closed() {
    let mut table = NameTable::default();
    let first = table.intern("client").unwrap();
    assert_eq!(table.intern("client"), Ok(first));
    assert_eq!(table.resolve(first), Some("client"));
    assert_eq!(table.resolve(NameId(u32::MAX)), None);
}

#[test]
fn exhaustion_is_explicit_and_does_not_forge_an_identity() {
    let mut table = NameTable::with_max_entries(1);
    assert!(table.intern("first").is_ok());
    assert_eq!(
        table.intern("second"),
        Err(NameExhausted {
            limit: 1,
            attempted: 2,
        })
    );
    assert_eq!(table.resolve(NameId(1)), None);
}

#[test]
fn lookup_miss_returns_none() {
    let table = NameTable::default();
    assert_eq!(table.lookup("nonexistent"), None);
}

#[test]
fn with_max_entries_boundary() {
    let mut table = NameTable::with_max_entries(0);
    assert!(table.intern("anything").is_err());
    assert!(table.exhausted());
}

#[test]
fn exhaustion_tracks_limit() {
    let mut table = NameTable::with_max_entries(2);
    table.intern("a").unwrap();
    table.intern("b").unwrap();
    let err = table.intern("c").unwrap_err();
    assert_eq!(err.limit, 2);
    assert_eq!(err.attempted, 3);
}

#[test]
fn lookup_returns_existing_id() {
    let mut table = NameTable::default();
    let id = table.intern("existing").unwrap();
    assert_eq!(table.lookup("existing"), Some(id));
}

#[test]
fn multiple_names_get_unique_ids() {
    let mut table = NameTable::default();
    let a = table.intern("alpha").unwrap();
    let b = table.intern("beta").unwrap();
    let c = table.intern("gamma").unwrap();
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
    assert_eq!(table.len(), 3);
}

#[test]
fn resolve_nonexistent_id_returns_none() {
    let table = NameTable::default();
    assert_eq!(table.resolve(NameId(0)), None);
    assert_eq!(table.resolve(NameId(1)), None);
    assert_eq!(table.resolve(NameId(u32::MAX)), None);
}

#[test]
fn is_empty_on_fresh_table() {
    let table = NameTable::default();
    assert!(table.is_empty());
}

#[test]
fn is_empty_after_insert() {
    let mut table = NameTable::default();
    table.intern("x").unwrap();
    assert!(!table.is_empty());
}

#[test]
fn len_counts_uniquely() {
    let mut table = NameTable::default();
    assert_eq!(table.len(), 0);
    table.intern("a").unwrap();
    assert_eq!(table.len(), 1);
    table.intern("a").unwrap();
    assert_eq!(table.len(), 1);
    table.intern("b").unwrap();
    assert_eq!(table.len(), 2);
}

#[test]
fn iter_yields_all_entries_in_insertion_order() {
    let mut table = NameTable::default();
    table.intern("first").unwrap();
    table.intern("second").unwrap();
    table.intern("third").unwrap();
    let entries: Vec<_> = table.iter().collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], (NameId(0), "first"));
    assert_eq!(entries[1], (NameId(1), "second"));
    assert_eq!(entries[2], (NameId(2), "third"));
}

#[test]
fn exhaustion_only_after_failure() {
    let mut table = NameTable::with_max_entries(2);
    assert!(!table.exhausted());
    assert!(table.exhaustion().is_none());
    table.intern("a").unwrap();
    assert!(!table.exhausted());
    table.intern("b").unwrap();
    assert!(!table.exhausted());
    table.intern("c").unwrap_err();
    assert!(table.exhausted());
    assert!(table.exhaustion().is_some());
}

#[test]
fn exhaustion_info_matches() {
    let mut table = NameTable::with_max_entries(2);
    table.intern("a").unwrap();
    table.intern("b").unwrap();
    let err = table.intern("c").unwrap_err();
    assert_eq!(err.limit, 2);
    assert_eq!(err.attempted, 3);
}

#[test]
fn name_id_debug_and_copy() {
    let id = NameId(42);
    let id2 = id;
    assert_eq!(format!("{id:?}"), "NameId(42)");
    assert_eq!(id, id2);
}

#[test]
fn name_exhausted_debug_and_copy() {
    let e = NameExhausted {
        limit: 10,
        attempted: 11,
    };
    let e2 = e;
    assert_eq!(
        format!("{e:?}"),
        "NameExhausted { limit: 10, attempted: 11 }"
    );
    assert_eq!(e, e2);
}
