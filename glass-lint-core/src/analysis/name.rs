//! Bounded names owned by one semantic artifact.

use std::{cell::RefCell, rc::Rc};

use indexmap::IndexSet;
use smol_str::{SmolStr, ToSmolStr};

/// Core bound for one artifact; it matches the default semantic-operation
/// bound while remaining independent of process lifetime and scheduling.
const MAX_NAMES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// An artifact-local name identity. It is meaningful only with its table.
pub(in crate::analysis) struct NameId(u32);

impl NameId {
    pub(in crate::analysis) const INVALID: Self = Self(u32::MAX);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct NameExhausted;

#[derive(Clone, Debug)]
/// Deterministic, bounded table of canonical semantic names for one artifact.
pub(in crate::analysis) struct NameTable {
    names: IndexSet<SmolStr>,
    max_entries: usize,
    exhausted: bool,
}

/// Private handle used only while one artifact is being lowered. The sharing
/// mechanism is deliberately hidden so callers cannot retain or compare
/// interner state outside the lowering lifetime.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct NameTableHandle(Rc<RefCell<NameTable>>);

impl Default for NameTableHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl NameTableHandle {
    pub(in crate::analysis) fn new() -> Self {
        Self(Rc::new(RefCell::new(NameTable::default())))
    }

    pub(in crate::analysis) fn intern(&self, name: &str) -> Result<NameId, NameExhausted> {
        self.0.borrow_mut().intern(name)
    }

    pub(in crate::analysis) fn lookup(&self, name: &str) -> Option<NameId> {
        self.0.borrow().lookup(name)
    }

    pub(in crate::analysis) fn exhausted(&self) -> bool {
        self.0.borrow().exhausted()
    }

    pub(in crate::analysis) fn with_mut<R>(&self, f: impl FnOnce(&mut NameTable) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    pub(in crate::analysis) fn into_table(self) -> NameTable {
        Rc::try_unwrap(self.0)
            .expect("name table handles must be released before freezing")
            .into_inner()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn snapshot(&self) -> NameTable {
        self.0.borrow().clone()
    }
}

impl NameTable {
    pub(crate) fn intern(&mut self, name: &str) -> Result<NameId, NameExhausted> {
        if let Some(index) = self.names.get_index_of(name) {
            return u32::try_from(index).map(NameId).map_err(|_| NameExhausted);
        }
        if self.names.len() >= self.max_entries {
            self.exhausted = true;
            return Err(NameExhausted);
        }
        let id = NameId(u32::try_from(self.names.len()).map_err(|_| {
            self.exhausted = true;
            NameExhausted
        })?);
        self.names.insert(name.to_smolstr());
        Ok(id)
    }

    // The first slice stores IDs before a textual matcher lookup needs this
    // conversion; keep the checked boundary here rather than exposing table
    // storage or a spelling fallback.
    #[allow(dead_code)]
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

    pub(in crate::analysis) fn exhausted(&self) -> bool {
        self.exhausted
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

#[cfg(test)]
impl NameTable {
    fn with_max_entries(max_entries: usize) -> Self {
        Self {
            names: IndexSet::new(),
            max_entries,
            exhausted: false,
        }
    }
}

#[cfg(test)]
impl From<&str> for NameId {
    fn from(name: &str) -> Self {
        // Path-interner unit tests construct paths without a lowering
        // context; this stable test-only spelling makes those tests concise.
        let mut hash = 2_166_136_261u32;
        for byte in name.bytes() {
            hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
        }
        Self(hash)
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
        assert_eq!(table.resolve(NameId::INVALID), None);
    }

    #[test]
    fn exhaustion_is_explicit_and_does_not_forge_an_identity() {
        let mut table = NameTable::with_max_entries(1);
        assert!(table.intern("first").is_ok());
        assert_eq!(table.intern("second"), Err(NameExhausted));
        assert_eq!(table.resolve(NameId(1)), None);
    }
}
