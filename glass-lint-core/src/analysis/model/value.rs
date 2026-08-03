use glass_lint_datastructures::{FastIndexSet, NameId, NamePath, NameTable, PathSegment};
use smol_str::SmolStr;

use crate::analysis::model::{
    StaticProperties,
    scope::{BindingKey, BindingSlot, FunctionId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(u32);

impl ValueId {
    pub const UNKNOWN: Self = Self::new(0);

    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(in crate::analysis) const fn raw(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(u32);

impl ObjectId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

/// Opaque collection of a static object's property/value pairs.
///
/// Property lookup, path traversal, and stable iteration live on this type so
/// matchers, summary projection, and constant conversion do not reimplement
/// raw tuple-slice logic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StaticObject {
    entries: StaticProperties<ValueId>,
}

impl StaticObject {
    /// Build a static object from source-ordered properties. Duplicate keys
    /// keep the last written value; a shape with more distinct properties than
    /// the collection bound is `None` (over budget, mapped to `Unknown`).
    pub fn new(entries: impl IntoIterator<Item = (NameId, ValueId)>) -> Option<Self> {
        let mut properties = StaticProperties::new();
        for (name, value) in entries {
            if !properties.insert(name, value) {
                return None;
            }
        }
        Some(Self {
            entries: properties,
        })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up the value bound to an interned property name.
    pub fn get(&self, name: NameId) -> Option<ValueId> {
        self.entries.get(name).copied()
    }

    pub fn contains_key(&self, name: NameId) -> bool {
        self.entries.contains_key(name)
    }

    /// Iterate `(NameId, ValueId)` pairs in deterministic source order.
    pub fn iter(&self) -> impl Iterator<Item = (NameId, ValueId)> + '_ {
        self.entries.iter().map(|(name, value)| (name, *value))
    }

    /// Advance a path traversal by one segment. Property segments resolve to
    /// the bound value; index segments cannot address an object property.
    pub fn value_at_segment(&self, segment: PathSegment) -> Option<ValueId> {
        match segment {
            PathSegment::Property(name) => self.get(name),
            PathSegment::Index(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Unknown,
    Global(SmolStr),
    Local,
    RootedMember { path: NamePath },
    ModuleNamespace(SmolStr),
    ModuleExport { module: SmolStr, export: SmolStr },
    StaticString(String),
    StaticNumber(usize),
    StaticArray(Vec<ValueId>),
    StaticObject(StaticObject),
    Callable(CallableValue),
    Object(ObjectId),
    Binding { key: BindingKey, target: ValueId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallableValue {
    target: ValueId,
    receiver: Option<ValueId>,
    bound_arguments: Vec<ValueId>,
}

impl CallableValue {
    pub fn new(target: ValueId, receiver: Option<ValueId>, bound_arguments: Vec<ValueId>) -> Self {
        Self {
            target,
            receiver,
            bound_arguments,
        }
    }

    pub fn target(&self) -> ValueId {
        self.target
    }
}

pub const MAX_VALUES: usize = 65_536;

#[derive(Debug, Clone)]
pub struct ValueTable {
    values: FastIndexSet<Value>,
    next_object: u32,
    exhausted: bool,
    terminal_cache: Vec<ValueId>,
}

impl Default for ValueTable {
    fn default() -> Self {
        Self {
            values: core::iter::once(Value::Unknown).collect(),
            next_object: 0,
            exhausted: false,
            terminal_cache: vec![ValueId::UNKNOWN],
        }
    }
}

impl ValueTable {
    pub fn intern(&mut self, value: Value) -> ValueId {
        let binding_target = match &value {
            Value::Binding { target, .. } => Some(*target),
            _ => None,
        };

        let (idx, inserted) = self.values.insert_full(value);
        let Ok(index) = u32::try_from(idx) else {
            if inserted {
                self.values.pop();
            }
            self.exhausted = true;
            return ValueId::UNKNOWN;
        };
        if !inserted {
            return ValueId::new(index);
        }
        if idx >= MAX_VALUES {
            self.values.pop();
            self.exhausted = true;
            return ValueId::UNKNOWN;
        }

        if let Some(target) = binding_target {
            if target.0 >= index {
                self.values.pop();
                self.exhausted = true;
                return ValueId::UNKNOWN;
            }
            self.terminal_cache
                .push(self.terminal_cache[usize::try_from(target.raw()).unwrap()]);
        } else {
            self.terminal_cache.push(ValueId::new(index));
        }

        ValueId::new(index)
    }

    pub fn intern_with_binding(&mut self, value: Value, binding: Option<BindingKey>) -> ValueId {
        let target = self.intern(value);
        binding.map_or(target, |key| self.intern(Value::Binding { key, target }))
    }

    pub fn intern_static_object(
        &mut self,
        values: impl IntoIterator<Item = (SmolStr, ValueId)>,
        names: &NameTable,
    ) -> ValueId {
        let mut canonical = Vec::new();
        for (name, value) in values {
            let Some(id) = names.lookup(name.as_str()) else {
                self.exhausted = true;
                return ValueId::UNKNOWN;
            };
            canonical.push((id, value));
        }
        let Some(object) = StaticObject::new(canonical) else {
            return ValueId::UNKNOWN;
        };
        self.intern(Value::StaticObject(object))
    }

    pub fn allocate_object_id(&mut self) -> Option<ObjectId> {
        const MAX_OBJECTS: u32 = 65_536;
        if self.next_object >= MAX_OBJECTS {
            self.exhausted = true;
            return None;
        }
        let object = ObjectId::new(self.next_object);
        self.next_object += 1;
        Some(object)
    }

    pub fn get(&self, id: ValueId) -> Option<&Value> {
        self.values.get_index(usize::try_from(id.raw()).ok()?)
    }

    pub fn resolve(&self, id: ValueId) -> Option<&Value> {
        let terminal = self.resolve_terminal(id)?;
        self.get(terminal)
    }

    pub fn resolve_id(&self, id: ValueId) -> Option<ValueId> {
        self.resolve_terminal(id)
    }

    pub fn binding_slot(&self, id: ValueId) -> Option<BindingSlot> {
        match self.get(id)? {
            Value::Binding { key, .. } => key.binding_slot(),
            _ => None,
        }
    }

    fn resolve_terminal(&self, id: ValueId) -> Option<ValueId> {
        let idx = usize::try_from(id.raw()).ok()?;
        self.terminal_cache.get(idx).copied()
    }

    pub fn static_string(&self, id: ValueId) -> Option<&str> {
        match self.resolve(id)? {
            Value::StaticString(value) => Some(value),
            _ => None,
        }
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted
    }
}

impl glass_lint_datastructures::IdIndex for FunctionId {
    fn from_raw(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_value_ids_fail_closed() {
        let arena = ValueTable::default();
        assert!(arena.get(ValueId::from_test(u32::MAX)).is_none());
        assert!(arena.get(ValueId::UNKNOWN).is_some());
    }

    #[test]
    fn value_capacity_is_typed_as_exhaustion() {
        let mut table = ValueTable::default();
        for index in 0..MAX_VALUES {
            let _ = table.intern(Value::StaticNumber(index));
        }
        assert!(table.exhausted());
        assert_eq!(
            table.intern(Value::StaticNumber(MAX_VALUES + 1)),
            ValueId::UNKNOWN
        );
    }

    #[test]
    fn callable_value_constructs_and_exposes_target() {
        let target = ValueId::from_test(42);
        let receiver = ValueId::from_test(7);
        let args = vec![ValueId::from_test(1), ValueId::from_test(2)];
        let cv = CallableValue::new(target, Some(receiver), args);
        assert_eq!(cv.target(), target);
    }

    #[test]
    fn intern_with_binding_wraps_in_binding_when_key_provided() {
        let mut table = ValueTable::default();
        let inner = table.intern(Value::StaticString("hello".into()));
        let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
            function: FunctionId::from_test(0),
            binding: crate::analysis::model::scope::BindingId::from_test(1),
            version: crate::analysis::model::scope::BindingVersion::from_test(0),
        });
        let wrapped = table.intern_with_binding(Value::StaticString("hello".into()), Some(key));
        assert_ne!(wrapped, inner);
        assert!(matches!(table.get(wrapped), Some(Value::Binding { .. })));
    }

    #[test]
    fn intern_with_binding_returns_direct_id_when_no_binding() {
        let mut table = ValueTable::default();
        let id = table.intern_with_binding(Value::StaticNumber(99), None);
        assert!(matches!(table.get(id), Some(Value::StaticNumber(99))));
    }

    #[test]
    fn resolve_follows_binding_chain_to_terminal_value() {
        let mut table = ValueTable::default();
        let terminal = table.intern(Value::StaticString("target".into()));
        let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
            function: FunctionId::from_test(0),
            binding: crate::analysis::model::scope::BindingId::from_test(0),
            version: crate::analysis::model::scope::BindingVersion::from_test(0),
        });
        let binding = table.intern(Value::Binding {
            key,
            target: terminal,
        });
        let resolved = table.resolve(binding);
        assert_eq!(resolved, Some(&Value::StaticString("target".into())));
    }

    #[test]
    fn resolve_follows_long_chain() {
        let mut table = ValueTable::default();
        let terminal = table.intern(Value::StaticString("target".into()));
        let mut prev = terminal;
        for i in 1..=20 {
            let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
                function: FunctionId::from_test(0),
                binding: crate::analysis::model::scope::BindingId::from_test(i),
                version: crate::analysis::model::scope::BindingVersion::from_test(0),
            });
            prev = table.intern(Value::Binding { key, target: prev });
        }
        assert_eq!(
            table.resolve(prev),
            Some(&Value::StaticString("target".into()))
        );
    }

    #[test]
    fn resolve_returns_terminal_for_non_binding_value() {
        let mut table = ValueTable::default();
        let id = table.intern(Value::StaticString("direct".into()));
        assert_eq!(
            table.resolve(id),
            Some(&Value::StaticString("direct".into()))
        );
    }

    #[test]
    fn resolve_returns_none_for_unknown_id() {
        let table = ValueTable::default();
        assert!(table.resolve(ValueId::from_test(u32::MAX)).is_none());
    }

    #[test]
    fn binding_slot_returns_named_slot_for_binding_values() {
        let mut table = ValueTable::default();
        let target = table.intern(Value::StaticString("target".into()));
        let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
            function: FunctionId::from_test(0),
            binding: crate::analysis::model::scope::BindingId::from_test(0),
            version: crate::analysis::model::scope::BindingVersion::from_test(0),
        });
        let binding = table.intern(Value::Binding { key, target });
        assert_eq!(
            table.binding_slot(binding),
            Some(BindingSlot::new(
                FunctionId::from_test(0),
                crate::analysis::model::scope::BindingId::from_test(0),
                NamePath::new(),
            ))
        );
        assert!(table.binding_slot(target).is_none());
    }

    #[test]
    fn static_string_returns_string_for_static_string_value() {
        let mut table = ValueTable::default();
        let id = table.intern(Value::StaticString("extracted".into()));
        assert_eq!(table.static_string(id), Some("extracted"));
    }

    #[test]
    fn static_string_returns_none_for_non_string_value() {
        let mut table = ValueTable::default();
        let id = table.intern(Value::StaticNumber(42));
        assert!(table.static_string(id).is_none());
    }

    #[test]
    fn static_string_follows_binding_chain() {
        let mut table = ValueTable::default();
        let target = table.intern(Value::StaticString("chained".into()));
        let key = BindingKey::new(crate::analysis::model::scope::BindingRoot::Binding {
            function: FunctionId::from_test(0),
            binding: crate::analysis::model::scope::BindingId::from_test(0),
            version: crate::analysis::model::scope::BindingVersion::from_test(0),
        });
        let binding = table.intern(Value::Binding { key, target });
        assert_eq!(table.static_string(binding), Some("chained"));
    }

    #[test]
    fn intern_static_object_creates_object_with_canonical_names() {
        let mut table = ValueTable::default();
        let mut names = NameTable::default();
        let key_a = names.intern("a").unwrap();
        let key_b = names.intern("b").unwrap();
        let val_a = table.intern(Value::StaticString("val_a".into()));
        let val_b = table.intern(Value::StaticNumber(1));
        let pairs = vec![("b".into(), val_b), ("a".into(), val_a)];
        let obj = table.intern_static_object(pairs, &names);
        let value = table.get(obj).expect("object should exist");
        let Value::StaticObject(object) = value else {
            panic!("expected StaticObject, got {value:?}");
        };
        assert_eq!(object.len(), 2);
        assert!(object.iter().any(|(k, _)| k == key_a));
        assert!(object.iter().any(|(k, _)| k == key_b));
    }

    #[test]
    fn intern_static_object_exhausts_on_unknown_name() {
        let mut table = ValueTable::default();
        let names = NameTable::default();
        let val = table.intern(Value::StaticNumber(0));
        let pairs = vec![("unknown".into(), val)];
        let result = table.intern_static_object(pairs, &names);
        assert_eq!(result, ValueId::UNKNOWN);
        assert!(table.exhausted());
    }

    #[test]
    fn allocate_object_id_returns_increasing_ids() {
        let mut table = ValueTable::default();
        let a = table.allocate_object_id().expect("first id");
        let b = table.allocate_object_id().expect("second id");
        assert_eq!(ObjectId::from_test(0), a);
        assert_eq!(ObjectId::from_test(1), b);
    }

    #[test]
    fn allocate_object_id_exhausts_at_max() {
        let mut table = ValueTable::default();
        for _ in 0..65_536 {
            table.allocate_object_id();
        }
        assert!(table.allocate_object_id().is_none());
        assert!(table.exhausted());
    }

    #[test]
    fn value_id_unknown_is_zero() {
        assert_eq!(ValueId::UNKNOWN, ValueId::from_test(0));
    }

    #[test]
    fn static_object_looks_up_properties_and_iterates_stably() {
        let mut names = NameTable::default();
        let key_a = names.intern("a").unwrap();
        let key_b = names.intern("b").unwrap();
        let mut table = ValueTable::default();
        let val_a = table.intern(Value::StaticString("a".into()));
        let val_b = table.intern(Value::StaticString("b".into()));
        let object =
            StaticObject::new(vec![(key_a, val_a), (key_b, val_b)]).expect("object fits budget");

        assert_eq!(object.len(), 2);
        assert!(!object.is_empty());
        assert_eq!(object.get(key_a), Some(val_a));
        assert_eq!(object.get(key_b), Some(val_b));
        assert_eq!(object.get(names.intern("missing").unwrap()), None);
        assert!(object.contains_key(key_a));
        assert!(!object.contains_key(names.intern("missing").unwrap()));

        let pairs: Vec<_> = object.iter().collect();
        assert_eq!(pairs, vec![(key_a, val_a), (key_b, val_b)]);
    }

    #[test]
    fn static_object_path_traversal_follows_properties_only() {
        let mut names = NameTable::default();
        let key_a = names.intern("a").unwrap();
        let key_b = names.intern("b").unwrap();
        let mut table = ValueTable::default();
        let val_a = table.intern(Value::StaticString("a".into()));
        let val_b = table.intern(Value::StaticString("b".into()));
        let object =
            StaticObject::new(vec![(key_a, val_a), (key_b, val_b)]).expect("object fits budget");

        assert_eq!(
            object.value_at_segment(PathSegment::Property(key_a)),
            Some(val_a)
        );
        assert_eq!(
            object.value_at_segment(PathSegment::Property(key_b)),
            Some(val_b)
        );
        assert_eq!(
            object.value_at_segment(PathSegment::Property(names.intern("missing").unwrap())),
            None
        );
        assert_eq!(object.value_at_segment(PathSegment::Index(0)), None);
    }

    #[test]
    fn static_object_new_rejects_over_budget_shapes() {
        let mut names = NameTable::default();
        let mut table = ValueTable::default();
        let value = table.intern(Value::StaticString("v".into()));
        let mut entries = Vec::with_capacity(257);
        for index in 0..257 {
            entries.push((names.intern(&format!("key_{index}")).unwrap(), value));
        }
        assert!(
            StaticObject::new(entries).is_none(),
            "an object with more properties than the bound must be unknown"
        );
    }

    #[test]
    fn static_object_new_applies_last_write_wins_to_duplicates() {
        let mut names = NameTable::default();
        let key = names.intern("key").unwrap();
        let mut table = ValueTable::default();
        let first = table.intern(Value::StaticString("first".into()));
        let last = table.intern(Value::StaticString("last".into()));
        let object =
            StaticObject::new(vec![(key, first), (key, last)]).expect("object fits budget");
        assert_eq!(object.len(), 1);
        assert_eq!(object.get(key), Some(last));
    }

    #[test]
    fn value_debug_and_partial_eq() {
        let v1 = Value::StaticString("a".into());
        let v2 = Value::StaticString("a".into());
        let v3 = Value::StaticString("b".into());
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }
}
