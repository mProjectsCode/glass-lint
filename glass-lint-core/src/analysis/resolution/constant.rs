use super::*;

impl Resolver {
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
        let mut arena = self.values.borrow_mut();
        let target = arena.intern(value);
        binding.map_or(target, |key| arena.intern(Value::Binding { key, target }))
    }
}
