//! Constant value conversion for resolver-owned value identities.
//!
//! The query path holds an immutable arena borrow across the entire recursive
//! traversal so that static arrays, objects, and strings are never cloned into
//! intermediate state merely to inspect their variant or descendants.

use std::collections::BTreeMap;

use crate::analysis::resolution::{BindingKey, ConstValue, Resolver, Value, ValueId};

const MAX_CONST_DEPTH: usize = 32;

impl Resolver {
    /// Read a bounded constant value from the abstract value arena.
    ///
    /// The owned value arena remains stable across the entire recursive
    /// traversal because every nested call only performs immutable reads.
    /// Large static arrays and objects are visited by borrowed slice rather
    /// than cloned before inspection.
    pub(in crate::analysis) fn const_value(&self, id: ValueId) -> ConstValue {
        self.const_value_depth(id, 0)
    }

    fn const_value_depth(&self, id: ValueId, depth: usize) -> ConstValue {
        if depth >= MAX_CONST_DEPTH {
            return ConstValue::Unknown;
        }
        let values = &self.values;
        let Some(value) = values.resolve(id) else {
            return ConstValue::Unknown;
        };
        match value {
            Value::StaticString(s) => ConstValue::String(s.clone()),
            Value::StaticNumber(n) => ConstValue::NonNegativeInteger(*n),
            Value::StaticArray(ids) => {
                let children = ids
                    .iter()
                    .map(|&id| self.const_value_depth(id, depth + 1))
                    .collect();
                ConstValue::Array(children)
            }
            Value::StaticObject(entries) => {
                let mut object = BTreeMap::new();
                for &(name_id, value_id) in entries {
                    let Some(key) = self.scopes.resolve_name_id(name_id) else {
                        return ConstValue::Unknown;
                    };
                    object.insert(key, self.const_value_depth(value_id, depth + 1));
                }
                ConstValue::Object(object)
            }
            _ => ConstValue::Unknown,
        }
    }

    /// Intern a constant tree while preserving the optional binding identity.
    pub(in crate::analysis) fn intern_const_value(
        &mut self,
        value: ConstValue,
        binding: Option<BindingKey>,
    ) -> ValueId {
        let value = match value {
            ConstValue::Unknown => Value::Unknown,
            ConstValue::String(value) => Value::StaticString(value),
            ConstValue::NonNegativeInteger(value) => Value::StaticNumber(value),
            ConstValue::Array(values) => Value::StaticArray(
                values
                    .into_iter()
                    .map(|value| self.intern_const_value(value, None))
                    .collect(),
            ),
            ConstValue::Object(values) => {
                let values = values
                    .into_iter()
                    .map(|(key, value)| (key, self.intern_const_value(value, None)))
                    .collect::<Vec<_>>();
                let arena = &mut self.values;
                let id = arena.intern_static_object(values, self.scopes.name_table_mut());
                return binding.map_or(id, |key| arena.intern(Value::Binding { key, target: id }));
            }
        };
        self.values.intern_with_binding(value, binding)
    }
}
