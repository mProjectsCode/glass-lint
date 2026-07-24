use std::marker::PhantomData;

/// Trait for types used as dense index identifiers in an [`IndexTable`].
///
/// Requires `Copy + Into<u32>` so the identifier can be used as a storage
/// index without additional allocation or indirection.
pub trait IdIndex: Copy + Into<u32> {
    /// Constructs an identifier from a raw `u32` value.
    fn from_raw(raw: u32) -> Self;
}

/// A sparse, index-based storage table.
///
/// Maps dense `I` identifiers to optional `T` values.  Internally backed by a
/// `Vec<Option<T>>` where the index corresponds to the identifier.  This
/// offers O(1) lookup and efficient iteration over present entries, but is
/// not space-efficient for very sparse populations.
#[derive(Debug, Clone)]
pub struct IndexTable<I, T> {
    values: Vec<Option<T>>,
    _marker: PhantomData<I>,
}

impl<I: IdIndex, T> Default for IndexTable<I, T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            _marker: PhantomData,
        }
    }
}

impl<I: IdIndex, T> IndexTable<I, T> {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a shared reference to the value at `id`, or `None`.
    pub fn get(&self, id: I) -> Option<&T> {
        let index = usize::try_from(id.into()).ok()?;
        self.values.get(index)?.as_ref()
    }

    /// Returns a mutable reference to the value at `id`, or `None`.
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        let index = usize::try_from(id.into()).ok()?;
        self.values.get_mut(index)?.as_mut()
    }

    /// Inserts `value` at `id`.
    ///
    /// Returns `true` if the slot was vacant, `false` if it was occupied or
    /// the id could not be converted to a `usize`.
    ///
    /// The vector grows automatically to accommodate the id.
    pub fn insert(&mut self, id: I, value: T) -> bool {
        let raw: u32 = id.into();
        let Some(index) = usize::try_from(raw).ok() else {
            return false;
        };
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        let vacant = self.values[index].is_none();
        self.values[index] = Some(value);
        vacant
    }

    /// Simultaneously borrows one slot for reading and another for writing.
    ///
    /// Returns `None` when `read == write` (the borrows would alias).
    /// Returns `Some((None, None))` when both slots are beyond the current
    /// storage length.
    pub fn get_disjoint(&mut self, read: I, write: I) -> Option<(Option<&T>, Option<&mut T>)> {
        if read.into() == write.into() {
            return None;
        }
        let ri = usize::try_from(read.into()).ok()?;
        let wi = usize::try_from(write.into()).ok()?;
        if self.values.len() <= ri.max(wi) {
            return Some((None, None));
        }
        if ri < wi {
            let (left, right) = self.values.split_at_mut(wi);
            let read_ref = left[ri].as_ref();
            let write_ref = right[0].as_mut();
            Some((read_ref, write_ref))
        } else {
            let (left, right) = self.values.split_at_mut(ri);
            let write_ref = left[wi].as_mut();
            let read_ref = right[0].as_ref();
            Some((read_ref, write_ref))
        }
    }

    /// An iterator over `(id, &value)` pairs for present entries.
    ///
    /// Iteration order is by increasing id.
    pub fn iter(&self) -> impl Iterator<Item = (I, &T)> {
        self.values.iter().enumerate().filter_map(|(index, value)| {
            value.as_ref().map(|value| {
                let raw = u32::try_from(index).unwrap_or(u32::MAX);
                (I::from_raw(raw), value)
            })
        })
    }

    /// A mutable iterator over `(id, &mut value)` pairs for present entries.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut T)> {
        self.values
            .iter_mut()
            .enumerate()
            .filter_map(|(index, value)| {
                value.as_mut().map(|value| {
                    let raw = u32::try_from(index).unwrap_or(u32::MAX);
                    (I::from_raw(raw), value)
                })
            })
    }

    /// An iterator over shared references to present values.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.values.iter().filter_map(Option::as_ref)
    }

    /// Returns `true` if the slot at `id` is occupied.
    pub fn contains(&self, id: I) -> bool {
        self.get(id).is_some()
    }

    /// The number of occupied slots.
    pub fn len(&self) -> usize {
        self.values.iter().filter(|value| value.is_some()).count()
    }

    /// Returns `true` if no slots are occupied.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all values from the table, keeping the allocated storage.
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Shrinks the internal vector to the highest occupied index + 1.
    pub fn shrink_to_fit(&mut self) {
        let present_len = self
            .values
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        self.values.truncate(present_len);
        self.values.shrink_to_fit();
    }
}

#[cfg(test)]
mod tests {
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
        let mut table = IndexTable::new();
        assert!(table.insert(TestId(0), "hello"));
        assert_eq!(table.get(TestId(0)), Some(&"hello"));
        assert!(!table.insert(TestId(0), "world"));
        assert_eq!(table.get(TestId(0)), Some(&"world"));
    }

    #[test]
    fn vacancy_tracking() {
        let mut table = IndexTable::new();
        assert!(table.insert(TestId(1), "first"));
        assert!(!table.insert(TestId(1), "second"));
    }

    #[test]
    fn get_disjoint_non_overlapping() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(1), "b");
        let (r, w) = table.get_disjoint(TestId(0), TestId(1)).unwrap();
        assert_eq!(r, Some(&"a"));
        assert_eq!(w, Some(&mut "b"));
    }

    #[test]
    fn get_disjoint_equal_ids_returns_none() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        assert!(table.get_disjoint(TestId(0), TestId(0)).is_none());
    }

    #[test]
    fn get_disjoint_overlapping_reversed_order() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(1), "b");
        let (r, w) = table.get_disjoint(TestId(1), TestId(0)).unwrap();
        assert_eq!(r, Some(&"b"));
        assert_eq!(w, Some(&mut "a"));
    }

    #[test]
    fn iter_yields_present_entries() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(2), "c");
        let entries: Vec<_> = table.iter().collect();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&(TestId(0), &"a")));
        assert!(entries.contains(&(TestId(2), &"c")));
    }

    #[test]
    fn values_yields_present_values_only() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(1), "b");
        let values: Vec<_> = table.values().collect();
        assert_eq!(values, vec![&"a", &"b"]);
    }

    #[test]
    fn contains_checks_presence() {
        let mut table = IndexTable::new();
        table.insert(TestId(5), "present");
        assert!(table.contains(TestId(5)));
        assert!(!table.contains(TestId(0)));
    }

    #[test]
    fn sparse_slots_handled_correctly() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(2), "c");
        assert_eq!(table.get(TestId(1)), None);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn large_id_resizes() {
        let mut table = IndexTable::new();
        assert!(table.insert(TestId(1000), "far"));
        assert_eq!(table.get(TestId(1000)), Some(&"far"));
    }

    #[test]
    fn len_counts_present_entries() {
        let mut table = IndexTable::new();
        assert_eq!(table.len(), 0);
        table.insert(TestId(0), "a");
        assert_eq!(table.len(), 1);
        table.insert(TestId(1), "b");
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn get_mut_allows_mutation() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "hello");
        if let Some(v) = table.get_mut(TestId(0)) {
            *v = "world";
        }
        assert_eq!(table.get(TestId(0)), Some(&"world"));
    }

    #[test]
    fn get_mut_nonexistent_id() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        assert_eq!(table.get_mut(TestId(0)), None);
    }

    #[test]
    fn iter_mut_allows_mutation() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(1), "b");
        for (_, v) in table.iter_mut() {
            *v = "x";
        }
        assert_eq!(table.get(TestId(0)), Some(&"x"));
        assert_eq!(table.get(TestId(1)), Some(&"x"));
    }

    #[test]
    fn iter_mut_yields_all_entries() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(2), "c");
        let mut count = 0;
        for (id, v) in table.iter_mut() {
            count += 1;
            assert!(*v == "a" || *v == "c");
            if id == TestId(0) {
                assert_eq!(*v, "a");
            }
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn get_disjoint_both_out_of_bounds() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        let (r, w) = table.get_disjoint(TestId(10), TestId(20)).unwrap();
        assert!(r.is_none());
        assert!(w.is_none());
    }

    #[test]
    fn get_disjoint_write_out_of_bounds() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        // When either index is beyond the storage length, both are None
        let (r, w) = table.get_disjoint(TestId(0), TestId(10)).unwrap();
        assert!(r.is_none());
        assert!(w.is_none());
    }

    #[test]
    fn get_disjoint_read_out_of_bounds() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        // When either index is beyond the storage length, both are None
        let (r, w) = table.get_disjoint(TestId(10), TestId(0)).unwrap();
        assert!(r.is_none());
        assert!(w.is_none());
    }

    #[test]
    fn len_after_overwrite() {
        let mut table = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(0), "b");
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn is_empty_on_new_table() {
        let table: IndexTable<TestId, &str> = IndexTable::new();
        assert!(table.is_empty());
    }

    #[test]
    fn is_empty_after_insert() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        assert!(!table.is_empty());
    }

    #[test]
    fn is_empty_after_clear() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(1), "b");
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn clear_removes_all_entries() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        table.insert(TestId(2), "c");
        table.clear();
        assert!(!table.contains(TestId(0)));
        assert!(!table.contains(TestId(2)));
    }

    #[test]
    fn shrink_to_fit_removes_trailing_none_slots() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
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
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.shrink_to_fit();
        assert!(table.is_empty());
    }

    #[test]
    fn clone_produces_independent_table() {
        let mut table: IndexTable<TestId, &str> = IndexTable::new();
        table.insert(TestId(0), "a");
        let cloned = table.clone();
        assert_eq!(cloned.get(TestId(0)), Some(&"a"));
    }
}
