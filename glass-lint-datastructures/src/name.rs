use hashbrown::DefaultHashBuilder;
use smol_str::{SmolStr, ToSmolStr};

use crate::{
    FastIndexSet,
    path::{NamePath, SymbolPath},
};

/// The default maximum number of names in a [`NameTable`].
pub const DEFAULT_MAX_NAMES: usize = 1 << 20;

/// Opaque identifier for an interned name.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NameId(pub(crate) u32);

/// Error returned when the name table hits its maximum capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NameExhausted {
    pub limit: usize,
    pub attempted: usize,
}

/// A bidirectional mapping between human-readable names and compact
/// [`NameId`]s.
///
/// Uses an [`indexmap::IndexSet`] so both intern (name → id) and resolve (id →
/// name) are O(1) average case.  Names are stored as [`SmolStr`] for
/// small-string optimisation.
#[derive(Clone, Debug)]
pub struct NameTable {
    names: FastIndexSet<SmolStr>,
    max_entries: usize,
    exhausted: bool,
}

impl NameTable {
    /// Interns `name`, returning its stable [`NameId`].
    ///
    /// Returns `Err(NameExhausted)` when the table has reached its capacity
    /// limit.
    pub fn intern(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        if let Some(idx) = self.names.get_index_of(name) {
            let Ok(id) = u32::try_from(idx).map(NameId) else {
                self.exhausted = true;
                return Err(NameExhausted {
                    limit: self.max_entries,
                    attempted: idx.saturating_add(1),
                });
            };
            return Ok(id);
        }
        if self.names.len() >= self.max_entries {
            self.exhausted = true;
            return Err(NameExhausted {
                limit: self.max_entries,
                attempted: self.names.len().saturating_add(1),
            });
        }
        let (idx, _) = self.names.insert_full(name.to_smolstr());
        let Ok(id) = u32::try_from(idx).map(NameId) else {
            self.names.pop();
            self.exhausted = true;
            return Err(NameExhausted {
                limit: self.max_entries,
                attempted: idx.saturating_add(1),
            });
        };
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

    /// Returns a `NameId` only if the raw value is within the table's range.
    pub fn checked_id(&self, raw: u32) -> Option<NameId> {
        let idx = usize::try_from(raw).ok()?;
        if idx < self.names.len() {
            Some(NameId(raw))
        } else {
            None
        }
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
            names: FastIndexSet::with_hasher(DefaultHashBuilder::default()),
            max_entries: DEFAULT_MAX_NAMES,
            exhausted: false,
        }
    }
}

impl NameTable {
    /// Creates a table with a custom capacity limit.
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            names: FastIndexSet::with_hasher(DefaultHashBuilder::default()),
            max_entries,
            exhausted: false,
        }
    }
}

#[cfg(test)]
mod tests;
