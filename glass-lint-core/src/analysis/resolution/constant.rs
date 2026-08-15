//! Constant value conversion for resolver-owned value identities.
//!
//! The query path holds an immutable arena borrow across the entire recursive
//! traversal so that static arrays, objects, and strings are never cloned into
//! intermediate state merely to inspect their variant or descendants.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::analysis::{
    resolution::{BindingKey, ConstValue, Resolver, Value, ValueId},
    syntax::constant::MAX_DEPTH,
};

impl Resolver<'_> {
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
        if depth >= MAX_DEPTH {
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
                ConstValue::array(children)
            }
            Value::StaticObject(object) => {
                let mut result = BTreeMap::new();
                for (name_id, value_id) in object.iter() {
                    let Some(key) = self.names.resolve(name_id).map(SmolStr::new) else {
                        return ConstValue::Unknown;
                    };
                    result.insert(key, self.const_value_depth(value_id, depth + 1));
                }
                ConstValue::object(result)
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
        self.intern_bounded_const_value(value.bounded(), binding)
    }

    /// Intern a tree after the public conversion boundary has admitted all of
    /// its depth, node, container, and string bounds.
    fn intern_bounded_const_value(
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
                    .map(|value| self.intern_bounded_const_value(value, None))
                    .collect(),
            ),
            ConstValue::Object(values) => {
                let values = values
                    .into_iter()
                    .map(|(key, value)| (key, self.intern_bounded_const_value(value, None)))
                    .collect::<Vec<_>>();
                return self
                    .values
                    .intern_static_object(values, &self.names, binding);
            }
        };
        self.values.intern_value_with_binding(value, binding)
    }
}
