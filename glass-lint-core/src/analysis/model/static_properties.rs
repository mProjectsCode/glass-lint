use glass_lint_datastructures::NameId;
use smol_str::SmolStr;

use crate::analysis::syntax::constant::ConstValue;

/// Maximum number of distinct properties one static object may retain.
///
/// This matches the constant evaluator's object-key budget so both literal
/// construction paths treat the same shapes as over-budget `Unknown`.
const MAX_STATIC_PROPERTIES: usize = 256;

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
        if self.entries.len() >= MAX_STATIC_PROPERTIES {
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
    pub(in crate::analysis) fn iter(&self) -> impl Iterator<Item = (NameId, &V)> + '_ {
        self.entries.iter().map(|(key, value)| (*key, value))
    }

    /// Project the property keys into a text-keyed constant object whose
    /// values are all `Unknown`. Keys that do not resolve to text are dropped.
    pub(in crate::analysis) fn to_const_object(
        &self,
        resolve_name: &impl Fn(NameId) -> Option<SmolStr>,
    ) -> ConstValue {
        ConstValue::Object(
            self.keys()
                .filter_map(resolve_name)
                .map(|key| (key, ConstValue::Unknown))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use glass_lint_datastructures::NameTable;

    use super::*;

    fn intern(names: &mut NameTable, text: &str) -> NameId {
        names.intern(text).unwrap()
    }

    #[test]
    fn duplicate_keys_keep_last_write_in_source_order() {
        let mut names = NameTable::default();
        let a = intern(&mut names, "a");
        let b = intern(&mut names, "b");
        let mut properties = StaticProperties::new();
        assert!(properties.insert(a, 1));
        assert!(properties.insert(b, 2));
        assert!(properties.insert(a, 3));
        assert_eq!(properties.get(a), Some(&3));
        assert_eq!(properties.get(b), Some(&2));
        assert_eq!(properties.len(), 2);
        assert!(!properties.is_empty());
        assert!(properties.contains_key(a));
        assert!(!properties.contains_key(intern(&mut names, "missing")));
        assert_eq!(properties.keys().collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(
            properties.iter().collect::<Vec<_>>(),
            vec![(a, &3), (b, &2)]
        );
    }

    #[test]
    fn bound_exhaustion_fails_new_insertion() {
        let mut names = NameTable::default();
        let mut properties = StaticProperties::new();
        for index in 0..MAX_STATIC_PROPERTIES {
            assert!(properties.insert(intern(&mut names, &format!("key_{index}")), index));
        }
        assert!(!properties.insert(intern(&mut names, "overflow"), 0));
        assert_eq!(properties.len(), MAX_STATIC_PROPERTIES);
    }

    #[test]
    fn bound_exhaustion_never_rejects_existing_key_replacement() {
        let mut names = NameTable::default();
        let a = intern(&mut names, "a");
        let mut properties = StaticProperties::new();
        assert!(properties.insert(a, 0));
        for index in 1..MAX_STATIC_PROPERTIES {
            assert!(properties.insert(intern(&mut names, &format!("key_{index}")), index));
        }
        assert!(properties.insert(a, 1));
        assert_eq!(properties.get(a), Some(&1));
        assert_eq!(properties.len(), MAX_STATIC_PROPERTIES);
    }

    #[test]
    fn to_const_object_projects_text_keys_with_unknown_values() {
        let mut names = NameTable::default();
        let mut properties = StaticProperties::new();
        properties.insert(intern(&mut names, "b"), 2);
        properties.insert(intern(&mut names, "a"), 1);
        let resolve = |key| names.resolve(key).map(SmolStr::new);
        assert_eq!(
            properties.to_const_object(&resolve),
            ConstValue::Object(BTreeMap::from([
                ("a".into(), ConstValue::Unknown),
                ("b".into(), ConstValue::Unknown),
            ]))
        );
    }
}
