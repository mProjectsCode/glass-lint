//! Interned abstract values and the bounded per-file arena.
//!
//! Equal abstract values share one `ValueId`, while `Binding` wrappers retain
//! the lexical version that made an observation valid at a source position.
//! The table never evicts entries, so IDs remain stable for the lifetime of a
//! file analysis.

use indexmap::IndexSet;
use smol_str::SmolStr;

use crate::analysis::{
    name::{NameId, NameTable},
    value::{BindingKey, NamePath, ValueId},
};

pub(in crate::analysis) const MAX_VALUES: usize = 65_536;
const MAX_OBJECTS: u32 = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Abstract value shape used by resolution and flow analysis.
pub(in crate::analysis) enum Value {
    /// No supported identity was proven.
    Unknown,
    /// A configured global root.
    Global(SmolStr),
    /// A local or ambiguous value.
    Local,
    /// A statically rooted member path.
    RootedMember { path: NamePath },
    /// A module namespace identity.
    ModuleNamespace(SmolStr),
    /// A named export identity.
    ModuleExport { module: SmolStr, export: SmolStr },
    /// A bounded static string value.
    StaticString(String),
    /// A bounded non-negative static number.
    StaticNumber(usize),
    /// An interned array of value identities.
    StaticArray(Vec<ValueId>),
    /// An interned object shape of named value identities.
    StaticObject(Vec<(NameId, ValueId)>),
    /// A callable target with receiver and bound arguments.
    Callable(CallableValue),
    /// An allocated object identity.
    Object(ObjectId),
    /// A value qualified by a lexical binding/version key.
    Binding { key: BindingKey, target: ValueId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Callable identity and its modeled receiver/bound argument values.
pub(in crate::analysis) struct CallableValue {
    /// Underlying callable value.
    target: ValueId,
    /// Receiver captured by a supported bind operation.
    receiver: Option<ValueId>,
    /// Static values captured after the receiver argument.
    bound_arguments: Vec<ValueId>,
}

impl CallableValue {
    /// Construct a canonical callable descriptor.
    pub(in crate::analysis) fn new(
        target: ValueId,
        receiver: Option<ValueId>,
        bound_arguments: Vec<ValueId>,
    ) -> Self {
        Self {
            target,
            receiver,
            bound_arguments,
        }
    }

    /// Return the underlying callable target.
    pub(in crate::analysis) fn target(&self) -> ValueId {
        self.target
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Bounded identity for an allocated object value.
pub(in crate::analysis) struct ObjectId(pub(in crate::analysis) u32);

#[derive(Debug, Clone)]
/// Per-file canonical value arena with explicit capacity limits.
pub(in crate::analysis) struct ValueTable {
    /// Insertion-ordered canonical storage. The set index is the value ID.
    values: IndexSet<Value>,
    /// Next bounded object identity.
    next_object: u32,
    exhausted: bool,
}

impl Default for ValueTable {
    fn default() -> Self {
        Self {
            values: IndexSet::from([Value::Unknown]),
            next_object: 0,
            exhausted: false,
        }
    }
}

impl ValueTable {
    /// Intern a value, returning unknown when the arena is exhausted.
    pub(in crate::analysis) fn intern(&mut self, value: Value) -> ValueId {
        if let Some(index) = self.values.get_index_of(&value) {
            return ValueId(u32::try_from(index).unwrap_or(ValueId::UNKNOWN.0));
        }
        if self.values.len() >= MAX_VALUES {
            self.exhausted = true;
            return ValueId::UNKNOWN;
        }
        let Ok(index) = u32::try_from(self.values.len()) else {
            return ValueId::UNKNOWN;
        };
        let id = ValueId(index);
        self.values.insert(value);
        id
    }

    /// Intern a value and, when present, preserve the binding identity that
    /// made the value observable at a particular source position.
    pub(in crate::analysis) fn intern_with_binding(
        &mut self,
        value: Value,
        binding: Option<BindingKey>,
    ) -> ValueId {
        let target = self.intern(value);
        binding.map_or(target, |key| self.intern(Value::Binding { key, target }))
    }

    pub(in crate::analysis) fn intern_static_object(
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
        self.intern(Value::StaticObject(canonical))
    }

    /// Allocate a distinct object identity within the object budget.
    pub(in crate::analysis) fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object >= MAX_OBJECTS {
            self.exhausted = true;
            return None;
        }
        let object = ObjectId(self.next_object);
        self.next_object += 1;
        Some(object)
    }

    /// Borrow an interned value, rejecting malformed/out-of-range IDs.
    pub(in crate::analysis) fn get(&self, id: ValueId) -> Option<&Value> {
        self.values.get_index(usize::try_from(id.0).ok()?)
    }

    /// Follow a bounded binding chain to the concrete value shape.
    pub(in crate::analysis) fn resolve(&self, id: ValueId) -> Option<&Value> {
        let mut value = self.get(id)?;
        for _ in 0..16 {
            match value {
                Value::Binding { target, .. } => value = self.get(*target)?,
                _ => return Some(value),
            }
        }
        None
    }

    /// Borrow a static string without materializing a duplicate projection.
    pub(in crate::analysis) fn static_string(&self, id: ValueId) -> Option<&str> {
        match self.resolve(id)? {
            Value::StaticString(value) => Some(value),
            _ => None,
        }
    }

    pub(in crate::analysis) fn exhausted(&self) -> bool {
        self.exhausted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
