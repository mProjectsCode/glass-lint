use glass_lint_datastructures::{NameId, NamePath, NameTable};
use indexmap::IndexSet;
use smol_str::SmolStr;

use crate::analysis::model::scope::{BindingKey, FunctionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

impl ValueId {
    pub const UNKNOWN: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u32);

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
    StaticObject(Vec<(NameId, ValueId)>),
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
    values: IndexSet<Value>,
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
    pub fn intern(&mut self, value: Value) -> ValueId {
        let (idx, inserted) = self.values.insert_full(value);
        let Ok(index) = u32::try_from(idx) else {
            if inserted {
                self.values.pop();
            }
            self.exhausted = true;
            return ValueId::UNKNOWN;
        };
        if !inserted {
            return ValueId(index);
        }
        if idx >= MAX_VALUES {
            self.values.pop();
            self.exhausted = true;
            return ValueId::UNKNOWN;
        }
        ValueId(index)
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
        self.intern(Value::StaticObject(canonical))
    }

    pub fn allocate_object_id(&mut self) -> Option<ObjectId> {
        const MAX_OBJECTS: u32 = 65_536;
        if self.next_object >= MAX_OBJECTS {
            self.exhausted = true;
            return None;
        }
        let object = ObjectId(self.next_object);
        self.next_object += 1;
        Some(object)
    }

    pub fn get(&self, id: ValueId) -> Option<&Value> {
        self.values.get_index(usize::try_from(id.0).ok()?)
    }

    pub fn resolve(&self, id: ValueId) -> Option<&Value> {
        let mut value = self.get(id)?;
        for _ in 0..16 {
            match value {
                Value::Binding { target, .. } => value = self.get(*target)?,
                _ => return Some(value),
            }
        }
        None
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
        Self(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_value_ids_fail_closed() {
        let arena = ValueTable::default();
        assert!(arena.get(ValueId(u32::MAX)).is_none());
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
}
