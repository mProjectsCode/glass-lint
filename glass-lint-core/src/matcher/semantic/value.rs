//! Interned abstract values used by the semantic resolver.
//!
//! Values are deliberately facts, not spellings.  In particular, a rooted
//! member and an exported module member remain distinct even when their source
//! text happens to look alike.

use std::{collections::HashMap, fmt};

const MAX_VALUES: usize = 65_536;
const MAX_OBJECTS: u32 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct ValueId(u32);

impl ValueId {
    pub(super) const UNKNOWN: Self = Self(0);
}

/// Stable lexical identity for one declaration in one parsed file.
///
/// These IDs are intentionally opaque to semantic consumers.  A source name
/// is only useful while collecting declarations; after that, identity is the
/// declaration plus its position-sensitive version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct BindingId(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct BindingVersion(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct FunctionId(pub(super) u32);

/// A canonical member path.  Path segments remain source symbols, but the
/// path itself is never used as a formatted string for identity comparisons.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct SymbolPath(Vec<String>);

impl SymbolPath {
    pub(super) fn from_chain(chain: &str) -> Self {
        Self(
            chain
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    pub(super) fn is_root(&self) -> bool {
        self.0.len() <= 1
    }

    pub(super) fn without_bind_suffix(&self) -> Option<Self> {
        self.0
            .last()
            .is_some_and(|segment| segment == "bind")
            .then(|| Self(self.0[..self.0.len().saturating_sub(1)].to_vec()))
    }

    pub(super) fn append_chain(&self, suffix: &str) -> Self {
        let mut path = self.0.clone();
        path.extend(
            suffix
                .strip_prefix('.')
                .unwrap_or(suffix)
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string),
        );
        Self(path)
    }
}

impl From<String> for SymbolPath {
    fn from(value: String) -> Self {
        Self::from_chain(&value)
    }
}

impl From<&str> for SymbolPath {
    fn from(value: &str) -> Self {
        Self::from_chain(value)
    }
}

impl fmt::Display for SymbolPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("."))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum BindingRoot {
    Binding {
        function: FunctionId,
        binding: BindingId,
        version: BindingVersion,
    },
    Global(String),
}

/// Identity of a binding or a property rooted in that binding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct BindingKey {
    pub(super) root: BindingRoot,
    pub(super) path: Vec<String>,
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
    /// A position-sensitive lexical value.  The target points at the
    /// canonical value while the key prevents two versions of one binding
    /// from collapsing into one fact.
    Binding {
        key: BindingKey,
        target: ValueId,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_value_ids_fail_closed() {
        let arena = ValueArena::default();
        assert!(arena.get(ValueId(u32::MAX)).is_none());
        assert!(arena.get(ValueId::UNKNOWN).is_some());
    }

    #[test]
    fn binding_versions_are_part_of_identity() {
        let first = BindingKey {
            root: BindingRoot::Binding {
                function: FunctionId(1),
                binding: BindingId(2),
                version: BindingVersion(0),
            },
            path: vec!["value".into()],
        };
        let second = BindingKey {
            root: BindingRoot::Binding {
                function: FunctionId(1),
                binding: BindingId(2),
                version: BindingVersion(1),
            },
            path: vec!["value".into()],
        };
        assert_ne!(first, second);
    }

    #[test]
    fn symbol_paths_keep_segments_out_of_identity_formatting() {
        let path = SymbolPath::from_chain("client.request").append_chain(".send");
        assert_eq!(path.to_string(), "client.request.send");
        assert!(!path.is_root());
        assert!(SymbolPath::from_chain("fetch").is_root());
        assert_eq!(
            SymbolPath::from_chain("fetch.bind")
                .without_bind_suffix()
                .expect("bind suffix should be removable")
                .to_string(),
            "fetch"
        );
    }
}
