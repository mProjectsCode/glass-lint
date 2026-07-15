//! Interned abstract values and the bounded per-file arena.

use super::{BindingKey, ValueId};
use std::collections::HashMap;

const MAX_VALUES: usize = 65_536;
const MAX_OBJECTS: u32 = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::analysis) enum Value {
    Unknown,
    Global(String),
    Local,
    RootedMember { root: String, path: Vec<String> },
    ModuleNamespace(String),
    ModuleExport { module: String, export: String },
    StaticString(String),
    StaticNumber(usize),
    StaticArray(Vec<ValueId>),
    StaticObject(Vec<(String, ValueId)>),
    Callable(CallableValue),
    Object(ObjectId),
    Binding { key: BindingKey, target: ValueId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::analysis) struct CallableValue {
    pub(in crate::analysis) target: ValueId,
    pub(in crate::analysis) receiver: Option<ValueId>,
    pub(in crate::analysis) bound_arguments: Vec<ValueId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct ObjectId(pub(in crate::analysis) u32);

#[derive(Debug)]
pub(in crate::analysis) struct ValueArena {
    values: Vec<Value>,
    ids: HashMap<Value, ValueId>,
    next_object: u32,
}

impl Default for ValueArena {
    fn default() -> Self {
        let mut ids = HashMap::new();
        ids.insert(Value::Unknown, ValueId::UNKNOWN);
        Self {
            values: vec![Value::Unknown],
            ids,
            next_object: 0,
        }
    }
}

impl ValueArena {
    pub(in crate::analysis) fn intern(&mut self, value: Value) -> ValueId {
        if let Some(id) = self.ids.get(&value) {
            return *id;
        }
        if self.values.len() >= MAX_VALUES {
            return ValueId::UNKNOWN;
        }
        let Ok(index) = u32::try_from(self.values.len()) else {
            return ValueId::UNKNOWN;
        };
        let id = ValueId(index);
        self.values.push(value.clone());
        self.ids.insert(value, id);
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
    pub(in crate::analysis) fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object >= MAX_OBJECTS {
            return None;
        }
        let object = ObjectId(self.next_object);
        self.next_object += 1;
        Some(object)
    }
    pub(in crate::analysis) fn get(&self, id: ValueId) -> Option<&Value> {
        self.values.get(usize::try_from(id.0).ok()?)
    }
}
