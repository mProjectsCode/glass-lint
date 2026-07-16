//! Constant value conversion for resolver-owned value identities.

use super::{BindingKey, ConstValue, Resolver, Value, ValueId};

impl Resolver {
    /// Read a bounded constant value from the abstract value arena.
    pub(in crate::analysis) fn const_value(&self, id: ValueId) -> ConstValue {
        let Some(value) = self.values.borrow().get(id).cloned() else {
            return ConstValue::Unknown;
        };
        match value {
            Value::Binding { target, .. } => self.const_value(target),
            Value::StaticString(value) => ConstValue::String(value),
            Value::StaticNumber(value) => ConstValue::NonNegativeInteger(value),
            Value::StaticArray(values) => {
                ConstValue::Array(values.into_iter().map(|id| self.const_value(id)).collect())
            }
            Value::StaticObject(values) => ConstValue::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, self.const_value(value)))
                    .collect(),
            ),
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
            ConstValue::Object(values) => Value::StaticObject(
                values
                    .into_iter()
                    .map(|(key, value)| (key, self.intern_const_value(value, None)))
                    .collect(),
            ),
        };
        self.values.borrow_mut().intern_with_binding(value, binding)
    }
}
