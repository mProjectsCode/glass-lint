//! Bounded names owned by one semantic artifact.

use std::cell::RefCell;

use indexmap::IndexSet;
use smol_str::{SmolStr, ToSmolStr};

use crate::analysis::value::{NamePath, SymbolPath};

/// Core bound for one artifact; it matches the default semantic-operation
/// bound while remaining independent of process lifetime and scheduling.
pub(in crate::analysis) const MAX_NAMES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// An artifact-local name identity.
///
/// `NameId` values are opaque and meaningful only with the `NameTable` that
/// produced them. They may be compared for equality within the same artifact
/// but must not be shared across artifacts or persisted. Textual ordering
/// and cross-artifact interfaces continue to use strings.
pub(in crate::analysis) struct NameId(pub(in crate::analysis) u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct NameExhausted {
    pub(in crate::analysis) limit: usize,
    pub(in crate::analysis) attempted: usize,
}

#[derive(Clone, Debug)]
/// Deterministic, bounded table of canonical semantic names for one artifact.
pub(in crate::analysis) struct NameTable {
    names: IndexSet<SmolStr>,
    max_entries: usize,
    exhausted: bool,
}

/// Borrowed context for interning and resolving names during lowering. The
/// lowering function owns the underlying `RefCell<NameTable>` and passes this
/// borrowed view to every phase. No `Rc`, no `try_unwrap`, no panic path.
#[derive(Copy, Clone, Debug)]
pub(in crate::analysis) struct NameTableCtx<'a>(pub(super) &'a RefCell<NameTable>);

#[allow(clippy::trivially_copy_pass_by_ref)]
impl NameTableCtx<'_> {
    pub(in crate::analysis) fn intern(&self, name: &str) -> Result<NameId, NameExhausted> {
        self.0.borrow_mut().intern(name)
    }

    pub(in crate::analysis) fn lookup(&self, name: &str) -> Option<NameId> {
        self.0.borrow().lookup(name)
    }

    pub(in crate::analysis) fn resolve(&self, id: NameId) -> Option<SmolStr> {
        self.0.borrow().resolve(id).map(SmolStr::new)
    }

    /// Intern every segment of a path. Failed segments produce `None`.
    pub(in crate::analysis) fn intern_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.0.borrow_mut().intern_path(path)
    }

    /// Look up an already-known path without mutating the table.
    pub(in crate::analysis) fn lookup_path(&self, path: &SymbolPath) -> Option<NamePath> {
        self.0.borrow().lookup_path(path)
    }

    /// Resolve an artifact-local path through the live table.
    pub(in crate::analysis) fn resolve_path(&self, path: &NamePath) -> Option<SymbolPath> {
        self.0.borrow().resolve_path(path)
    }

    pub(in crate::analysis) fn exhausted(&self) -> bool {
        self.0.borrow().exhausted()
    }

    pub(in crate::analysis) fn exhaustion(&self) -> Option<NameExhausted> {
        self.0.borrow().exhaustion()
    }

    /// Convenience constructor for test code that needs a throwaway context.
    /// The backing table is leaked to `'static` so callers don't need to own
    /// their own `RefCell` in single-use helper functions.
    #[cfg(test)]
    pub(in crate::analysis) fn testing() -> NameTableCtx<'static> {
        let cell = Box::new(RefCell::new(NameTable::default()));
        NameTableCtx(Box::leak(cell))
    }

    pub(in crate::analysis) fn with_mut<R>(&self, f: impl FnOnce(&mut NameTable) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    #[cfg(test)]
    pub(in crate::analysis) fn snapshot(&self) -> NameTable {
        self.0.borrow().clone()
    }
}

impl NameTable {
    pub(crate) fn intern(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        if let Some(index) = self.names.get_index_of(name) {
            return u32::try_from(index).map(NameId).map_err(|_| NameExhausted {
                limit: self.max_entries,
                attempted: index.saturating_add(1),
            });
        }
        if self.names.len() >= self.max_entries {
            self.exhausted = true;
            return Err(NameExhausted {
                limit: self.max_entries,
                attempted: self.names.len().saturating_add(1),
            });
        }
        let id = NameId(u32::try_from(self.names.len()).map_err(|_| {
            self.exhausted = true;
            NameExhausted {
                limit: self.max_entries,
                attempted: self.names.len().saturating_add(1),
            }
        })?);
        self.names.insert(name.to_smolstr());
        Ok(id)
    }

    // The first slice stores IDs before a textual matcher lookup needs this
    // conversion; keep the checked boundary here rather than exposing table
    // storage or a spelling fallback.
    pub(in crate::analysis) fn resolve(&self, id: NameId) -> Option<&str> {
        self.names
            .get_index(usize::try_from(id.0).ok()?)
            .map(SmolStr::as_str)
    }

    pub(in crate::analysis) fn lookup(&self, name: &str) -> Option<NameId> {
        self.names
            .get_index_of(name)
            .and_then(|index| u32::try_from(index).ok())
            .map(NameId)
    }

    fn intern_path(&mut self, path: &SymbolPath) -> Option<NamePath> {
        path.segments()
            .iter()
            .try_fold(NamePath::new(), |mut path, segment| {
                path.append(self.intern(segment).ok()?);
                Some(path)
            })
    }

    fn lookup_path(&self, path: &SymbolPath) -> Option<NamePath> {
        path.segments()
            .iter()
            .try_fold(NamePath::new(), |mut path, segment| {
                path.append(self.lookup(segment)?);
                Some(path)
            })
    }

    fn resolve_path(&self, path: &NamePath) -> Option<SymbolPath> {
        path.segments()
            .iter()
            .map(|id| self.resolve(*id).map(SmolStr::new))
            .collect::<Option<Vec<_>>>()
            .map(SymbolPath::from_segments)
    }

    pub(in crate::analysis) fn exhausted(&self) -> bool {
        self.exhausted
    }

    pub(in crate::analysis) fn exhaustion(&self) -> Option<NameExhausted> {
        self.exhausted.then_some(NameExhausted {
            limit: self.max_entries,
            attempted: self.names.len().saturating_add(1),
        })
    }
}

impl Default for NameTable {
    fn default() -> Self {
        Self {
            names: IndexSet::new(),
            max_entries: MAX_NAMES,
            exhausted: false,
        }
    }
}

impl NameTable {
    pub(in crate::analysis) fn with_max_entries(max_entries: usize) -> Self {
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
    fn borrowed_context_shares_names_without_rc() {
        let table = RefCell::new(NameTable::default());
        let ctx = NameTableCtx(&table);
        let first = ctx.intern("client").unwrap();
        assert_eq!(ctx.lookup("client"), Some(first));
        assert_eq!(ctx.resolve(first), Some(SmolStr::new("client")));
        let second = ctx.intern("client").unwrap();
        assert_eq!(first, second);
    }
}
