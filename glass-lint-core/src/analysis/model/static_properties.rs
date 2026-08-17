use glass_lint_datastructures::NameId;
use smol_str::SmolStr;

use crate::analysis::syntax::constant::{ConstValue, MAX_OBJECT_KEYS};

/// Opaque bounded static-property collection.
///
/// Insertion preserves source order and applies last-write-wins for duplicate
/// keys, matching JavaScript property overwrite semantics. Exceeding the
/// property bound fails the insertion and the caller maps the shape to
/// `Unknown` rather than retaining a partial or unbounded object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StaticProperties<V> {
    entries: Vec<(NameId, V)>,
}

impl<V> StaticProperties<V> {
    pub(in crate::analysis) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Insert one property in source order. A duplicate key replaces the
    /// earlier value in place (last write wins). Returns `false` when the
    /// distinct property count would exceed the collection bound.
    pub(in crate::analysis) fn insert(&mut self, name: NameId, value: V) -> bool {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(key, _)| *key == name) {
            *existing = value;
            return true;
        }
        if self.entries.len() >= MAX_OBJECT_KEYS {
            return false;
        }
        self.entries.push((name, value));
        true
    }

    pub(in crate::analysis) fn get(&self, name: NameId) -> Option<&V> {
        self.entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }

    pub(in crate::analysis) fn contains_key(&self, name: NameId) -> bool {
        self.get(name).is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate distinct property keys in source order (the first-occurrence
    /// position for keys written more than once).
    pub(in crate::analysis) fn keys(&self) -> impl Iterator<Item = NameId> + '_ {
        self.entries.iter().map(|(key, _)| *key)
    }

    /// Iterate `(NameId, &V)` pairs in source order.
    #[cfg(test)]
    pub(in crate::analysis) fn iter(&self) -> impl Iterator<Item = (NameId, &V)> + '_ {
        self.entries.iter().map(|(key, value)| (*key, value))
    }

    /// Project the property keys into a text-keyed constant object whose
    /// values are all `Unknown`. An unresolved key invalidates the object.
    pub(in crate::analysis) fn to_const_object(
        &self,
        resolve_name: &impl Fn(NameId) -> Option<SmolStr>,
    ) -> Option<ConstValue> {
        Some(ConstValue::object(
            self.keys()
                .map(|key| Some((resolve_name(key)?, ConstValue::Unknown)))
                .collect::<Option<_>>()?,
        ))
    }
}

#[cfg(test)]
mod tests;
