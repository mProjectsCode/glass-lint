use indexmap::IndexSet;
use smol_str::{SmolStr, ToSmolStr};

use crate::path::{NamePath, SymbolPath};

/// The default maximum number of names in a [`NameTable`].
pub const DEFAULT_MAX_NAMES: usize = 1 << 20;

/// Opaque identifier for an interned name.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NameId(pub u32);

/// Error returned when the name table hits its maximum capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NameExhausted {
    pub limit: usize,
    pub attempted: usize,
}

/// A bidirectional mapping between human-readable names and compact
/// [`NameId`]s.
///
/// Uses an [`IndexSet`] so both intern (name → id) and resolve (id → name)
/// are O(1) average case.  Names are stored as [`SmolStr`] for small-string
/// optimisation.
#[derive(Clone, Debug)]
pub struct NameTable {
    names: IndexSet<SmolStr>,
    max_entries: usize,
    exhausted: bool,
}

impl NameTable {
    /// Interns `name`, returning its stable [`NameId`].
    ///
    /// Returns `Err(NameExhausted)` when the table has reached its capacity
    /// limit.
    pub fn intern(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        let (idx, inserted) = self.names.insert_full(name.to_smolstr());
        let Ok(id) = u32::try_from(idx).map(NameId) else {
            if inserted {
                self.names.pop();
            }
            self.exhausted = true;
            return Err(NameExhausted {
                limit: self.max_entries,
                attempted: idx.saturating_add(1),
            });
        };
        if !inserted {
            return Ok(id);
        }
        if idx >= self.max_entries {
            self.names.pop();
            self.exhausted = true;
            return Err(NameExhausted {
                limit: self.max_entries,
                attempted: idx.saturating_add(1),
            });
        }
        Ok(id)
    }

    /// Resolves `id` back to the interned string, or `None` if the id is
    /// out of range.
    pub fn resolve(&self, id: NameId) -> Option<&str> {
        self.names
            .get_index(usize::try_from(id.0).ok()?)
            .map(SmolStr::as_str)
    }

    /// Looks up an already-interned name without inserting it.
    pub fn lookup(&self, name: &str) -> Option<NameId> {
        self.names
            .get_index_of(name)
            .and_then(|index| u32::try_from(index).ok())
            .map(NameId)
    }

    /// Converts a [`SymbolPath`] to a [`NamePath`] by looking up each segment.
    ///
    /// Returns `None` if any segment is not yet interned.
    pub fn lookup_path(&self, path: &SymbolPath) -> Option<NamePath> {
        path.segments()
            .iter()
            .try_fold(NamePath::new(), |mut path, segment| {
                path.append(self.lookup(segment)?);
                Some(path)
            })
    }

    /// Converts a [`NamePath`] to a [`SymbolPath`] by resolving each ID.
    ///
    /// Returns `None` if any ID is out of range.
    pub fn resolve_path(&self, path: &NamePath) -> Option<SymbolPath> {
        path.segments()
            .iter()
            .map(|id| self.resolve(*id).map(SmolStr::new))
            .collect::<Option<Vec<_>>>()
            .map(SymbolPath::from_segments)
    }

    /// Returns `true` if the table has been exhausted.
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    /// Returns exhaustion details if the table has been exhausted.
    pub fn exhaustion(&self) -> Option<NameExhausted> {
        self.exhausted.then_some(NameExhausted {
            limit: self.max_entries,
            attempted: self.names.len().saturating_add(1),
        })
    }

    /// The maximum number of entries before exhaustion.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// The current number of interned names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns `true` if no names have been interned.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// An iterator over `(id, name)` pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (NameId, &str)> {
        self.names.iter().enumerate().filter_map(|(i, name)| {
            let raw = u32::try_from(i).ok()?;
            Some((NameId(raw), name.as_str()))
        })
    }
}

impl Default for NameTable {
    fn default() -> Self {
        Self {
            names: IndexSet::new(),
            max_entries: DEFAULT_MAX_NAMES,
            exhausted: false,
        }
    }
}

impl NameTable {
    /// Creates a table with a custom capacity limit.
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            names: IndexSet::new(),
            max_entries,
            exhausted: false,
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn iter_on_empty_table() {
        let table = NameTable::default();
        assert_eq!(table.iter().count(), 0);
    }

    #[test]
    fn max_entries_accessor() {
        let table = NameTable::with_max_entries(100);
        assert_eq!(table.max_entries(), 100);
        assert_eq!(table.max_entries(), table.max_entries());
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
}
