use InsertOutcome::*;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestId(u32);

impl IdIndex for TestId {
    fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<TestId> for u32 {
    fn from(id: TestId) -> Self {
        id.0
    }
}

#[test]
fn get_insert_and_get_mut() {
    let mut table = IndexTable::new(1000);
    assert_eq!(table.insert(TestId(0), "hello"), Inserted);
    assert_eq!(table.get(TestId(0)), Some(&"hello"));
    assert_eq!(table.insert(TestId(0), "world"), Replaced);
    assert_eq!(table.get(TestId(0)), Some(&"world"));
}

#[test]
fn vacancy_tracking() {
    let mut table = IndexTable::new(1000);
    assert_eq!(table.insert(TestId(1), "first"), Inserted);
    assert_eq!(table.insert(TestId(1), "second"), Replaced);
}

#[test]
fn get_disjoint_non_overlapping() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(1), "b");
    let (r, w) = table.get_disjoint(TestId(0), TestId(1)).unwrap();
    assert_eq!(r, Some(&"a"));
    assert_eq!(w, Some(&mut "b"));
}

#[test]
fn get_disjoint_equal_ids_returns_none() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    assert!(table.get_disjoint(TestId(0), TestId(0)).is_none());
}

#[test]
fn get_disjoint_overlapping_reversed_order() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(1), "b");
    let (r, w) = table.get_disjoint(TestId(1), TestId(0)).unwrap();
    assert_eq!(r, Some(&"b"));
    assert_eq!(w, Some(&mut "a"));
}

#[test]
fn iter_yields_present_entries() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(2), "c");
    let entries: Vec<_> = table.iter().collect();
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&(TestId(0), &"a")));
    assert!(entries.contains(&(TestId(2), &"c")));
}

#[test]
fn values_yields_present_values_only() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(1), "b");
    let values: Vec<_> = table.values().collect();
    assert_eq!(values, vec![&"a", &"b"]);
}

#[test]
fn contains_checks_presence() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(5), "present");
    assert!(table.contains(TestId(5)));
    assert!(!table.contains(TestId(0)));
}

#[test]
fn sparse_slots_handled_correctly() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(2), "c");
    assert_eq!(table.get(TestId(1)), None);
    assert_eq!(table.len(), 2);
}

#[test]
fn large_id_resizes() {
    let mut table = IndexTable::new(2000);
    assert_eq!(table.insert(TestId(1000), "far"), Inserted);
    assert_eq!(table.get(TestId(1000)), Some(&"far"));
}

#[test]
fn len_counts_present_entries() {
    let mut table = IndexTable::new(1000);
    assert_eq!(table.len(), 0);
    table.insert(TestId(0), "a");
    assert_eq!(table.len(), 1);
    table.insert(TestId(1), "b");
    assert_eq!(table.len(), 2);
}

#[test]
fn get_mut_allows_mutation() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "hello");
    if let Some(v) = table.get_mut(TestId(0)) {
        *v = "world";
    }
    assert_eq!(table.get(TestId(0)), Some(&"world"));
}

#[test]
fn get_mut_nonexistent_id() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    assert_eq!(table.get_mut(TestId(0)), None);
}

#[test]
fn iter_mut_covers_all_entries_and_allows_mutation() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(2), "c");
    let mut seen = Vec::new();
    for (id, v) in table.iter_mut() {
        seen.push(id);
        *v = "x";
    }
    assert_eq!(seen.len(), 2);
    assert!(seen.contains(&TestId(0)));
    assert!(seen.contains(&TestId(2)));
    assert_eq!(table.get(TestId(0)), Some(&"x"));
    assert_eq!(table.get(TestId(2)), Some(&"x"));
}

#[test]
fn get_disjoint_both_out_of_bounds() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    let (r, w) = table.get_disjoint(TestId(10), TestId(20)).unwrap();
    assert!(r.is_none());
    assert!(w.is_none());
}

#[test]
fn get_disjoint_write_out_of_bounds() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    // When either index is beyond the storage length, both are None
    let (r, w) = table.get_disjoint(TestId(0), TestId(10)).unwrap();
    assert!(r.is_none());
    assert!(w.is_none());
}

#[test]
fn get_disjoint_read_out_of_bounds() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    // When either index is beyond the storage length, both are None
    let (r, w) = table.get_disjoint(TestId(10), TestId(0)).unwrap();
    assert!(r.is_none());
    assert!(w.is_none());
}

#[test]
fn len_after_overwrite() {
    let mut table = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(0), "b");
    assert_eq!(table.len(), 1);
}

#[test]
fn is_empty_on_new_table() {
    let table: IndexTable<TestId, &str> = IndexTable::new(1000);
    assert!(table.is_empty());
}

#[test]
fn is_empty_after_insert() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    assert!(!table.is_empty());
}

#[test]
fn is_empty_after_clear() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(1), "b");
    table.clear();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn clear_removes_all_entries() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    table.insert(TestId(2), "c");
    table.clear();
    assert!(!table.contains(TestId(0)));
    assert!(!table.contains(TestId(2)));
}

#[test]
fn shrink_to_fit_removes_trailing_none_slots() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    // After inserting at id 10, internal storage must be at least 11
    // entries.  shrink_to_fit should truncate to exactly 11.
    table.shrink_to_fit();
    // Verify that entries are still accessible after shrinking
    assert_eq!(table.get(TestId(0)), Some(&"a"));
    assert_eq!(table.len(), 1);
}

#[test]
fn shrink_to_fit_empty() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.shrink_to_fit();
    assert!(table.is_empty());
}

#[test]
fn clone_produces_independent_table() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(1000);
    table.insert(TestId(0), "a");
    let cloned = table.clone();
    assert_eq!(cloned.get(TestId(0)), Some(&"a"));
}

#[test]
fn insert_rejects_id_at_capacity() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(10);
    assert_eq!(table.insert(TestId(10), "overflow"), OutOfRange);
    assert_eq!(table.len(), 0);
}

#[test]
fn insert_rejects_id_beyond_capacity() {
    let mut table: IndexTable<TestId, &str> = IndexTable::new(10);
    assert_eq!(table.insert(TestId(u32::MAX), "far"), OutOfRange);
    assert_eq!(table.len(), 0);
}
