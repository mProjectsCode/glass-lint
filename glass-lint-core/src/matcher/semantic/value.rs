//! Interned abstract values used by the semantic resolver.
//!
//! Values are deliberately facts, not spellings.  In particular, a rooted
//! member and an exported module member remain distinct even when their source
//! text happens to look alike.

use std::collections::HashMap;

const MAX_VALUES: usize = 65_536;
const MAX_OBJECTS: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ValueId(u32);

impl ValueId {
    pub(super) const UNKNOWN: Self = Self(0);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Value {
    Unknown,
    /// An unshadowed global identifier, such as `fetch`.
    Global(String),
    /// A binding whose value cannot safely be followed.
    Local,
    /// A property path rooted in an unshadowed name or `this`.
    RootedMember {
        root: String,
        path: Vec<String>,
    },
    /// The namespace object returned by an import or `require`.
    ModuleNamespace(String),
    /// A named member of an imported module. This is distinct from a rooted
    /// member so module-provenance matchers never match lookalike locals.
    ModuleExport {
        module: String,
        export: String,
    },
    StaticString(String),
    StaticNumber(usize),
    StaticArray(Vec<ValueId>),
    StaticObject(Vec<(String, ValueId)>),
    Callable(CallableValue),
    Object(ObjectId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CallableValue {
    /// The callable before applying `.bind`; the receiver and bound arguments
    /// are kept separately so later resolution can preserve that distinction.
    pub(super) target: ValueId,
    pub(super) receiver: Option<ValueId>,
    pub(super) bound_arguments: Vec<ValueId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ObjectId(pub(super) u32);

/// A small, per-file value interner.  `Unknown` is always zero, allowing
/// consumers to cheaply fail closed without allocating an alternative set.
#[derive(Debug)]
pub(super) struct ValueArena {
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
    pub(super) fn intern(&mut self, value: Value) -> ValueId {
        if self.values.len() >= MAX_VALUES {
            return ValueId::UNKNOWN;
        }
        if let Some(id) = self.ids.get(&value) {
            return *id;
        }
        let Ok(index) = u32::try_from(self.values.len()) else {
            return ValueId::UNKNOWN;
        };
        let id = ValueId(index);
        self.values.push(value.clone());
        self.ids.insert(value, id);
        id
    }

    pub(super) fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object >= MAX_OBJECTS {
            return None;
        }
        let object = ObjectId(self.next_object);
        self.next_object += 1;
        Some(object)
    }

    pub(super) fn get(&self, id: ValueId) -> Option<&Value> {
        self.values.get(usize::try_from(id.0).ok()?)
    }
}
