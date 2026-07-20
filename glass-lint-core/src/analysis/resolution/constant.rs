//! Constant value conversion for resolver-owned value identities.

use smol_str::SmolStr;

use super::{BindingKey, ConstValue, Resolver, Value, ValueId};

impl Resolver {
    /// Read a bounded constant value from the abstract value arena.
    pub(in crate::analysis) fn const_value(&self, id: ValueId) -> ConstValue {
        let Some(value) = self.state.borrow().values.get(id).cloned() else {
            return ConstValue::Unknown;
        };
        match value {
            Value::Binding { target, .. } => self.const_value(target),
            Value::StaticString(value) => ConstValue::String(value),
            Value::StaticNumber(value) => ConstValue::NonNegativeInteger(value),
            Value::StaticArray(values) => {
                ConstValue::Array(values.into_iter().map(|id| self.const_value(id)).collect())
            }
            Value::StaticObject(values) => {
                let mut object = std::collections::BTreeMap::new();
                for (key, value) in values {
                    let Some(key) = self
                        .names
                        .with_mut(|names| names.resolve(key).map(SmolStr::new))
                    else {
                        return ConstValue::Unknown;
                    };
                    object.insert(key, self.const_value(value));
                }
                ConstValue::Object(object)
            }
            _ => ConstValue::Unknown,
        }
    }

    /// Intern a constant tree while preserving the optional binding identity.
    pub(in crate::analysis) fn intern_const_value(
        &self,
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
                let mut state = self.state.borrow_mut();
                let id = self
                    .names
                    .with_mut(|names| state.values.intern_static_object(values, names));
                return binding.map_or(id, |key| {
                    state.values.intern(Value::Binding { key, target: id })
                });
            }
        };
        self.state
            .borrow_mut()
            .values
            .intern_with_binding(value, binding)
    }
}
