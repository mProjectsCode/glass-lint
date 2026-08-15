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
pub struct ResolvedObjectId(u32);

impl ResolvedObjectId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

/// Identity allocated by one object-flow projection run. It is intentionally
/// distinct from [`ResolvedObjectId`], whose allocator belongs to `ValueTable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlowObjectId(u32);

impl FlowObjectId {
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
    ModuleExport { module: SmolStr, export: SmolStr },
    StaticString(String),
    StaticNumber(usize),
    StaticArray(Vec<ValueId>),
    StaticObject(StaticObject),
    Callable(CallableValue),
    Object(ResolvedObjectId),
    Binding { key: BindingKey, target: ValueId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallableValue {
    target: ValueId,
}

impl CallableValue {
    pub fn new(target: ValueId) -> Self {
        Self { target }
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
    fn intern_value(&mut self, value: Value) -> ValueId {
        let binding_terminal = match &value {
            Value::Binding { target, .. } => {
                let Some(target_index) = usize::try_from(target.raw()).ok() else {
                    self.exhausted = true;
                    return ValueId::UNKNOWN;
                };
                let Some(terminal) = self.terminal_cache.get(target_index).copied() else {
                    self.exhausted = true;
                    return ValueId::UNKNOWN;
                };
                Some(terminal)
            }
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

        if let Some(terminal) = binding_terminal {
            self.terminal_cache.push(terminal);
        } else {
            self.terminal_cache.push(ValueId::new(index));
        }

        ValueId::new(index)
    }

    pub(in crate::analysis) fn intern_value_with_binding(
        &mut self,
        value: Value,
        binding: Option<BindingKey>,
    ) -> ValueId {
        let target = self.intern_value(value);
        binding.map_or(target, |key| {
            self.intern_value(Value::Binding { key, target })
        })
    }

    pub(in crate::analysis) fn intern_static_object(
        &mut self,
        values: impl IntoIterator<Item = (SmolStr, ValueId)>,
        names: &NameTable,
        binding: Option<BindingKey>,
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
        self.intern_value_with_binding(Value::StaticObject(object), binding)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn intern(&mut self, value: Value) -> ValueId {
        self.intern_value(value)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn intern_with_binding(
        &mut self,
        value: Value,
        binding: Option<BindingKey>,
    ) -> ValueId {
        self.intern_value_with_binding(value, binding)
    }

    pub fn allocate_object_id(&mut self) -> Option<ResolvedObjectId> {
        const MAX_OBJECTS: u32 = 65_536;
        if self.next_object >= MAX_OBJECTS {
            self.exhausted = true;
            return None;
        }
        let object = ResolvedObjectId::new(self.next_object);
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
mod tests;
