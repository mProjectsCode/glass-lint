//! Constant value conversion for resolver-owned value identities.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::analysis::resolution::{BindingKey, ConstValue, Resolver, Value, ValueId};

/// Action extracted from a borrowed arena entry so the RefCell borrow
/// is released before any recursive traversal.
enum ConstAction {
    Binding(ValueId),
    String(String),
    Number(usize),
    Array(Vec<ValueId>),
    Object(Vec<(crate::analysis::name::NameId, ValueId)>),
    Unknown,
}

impl Resolver<'_> {
    /// Read a bounded constant value from the abstract value arena without
    /// cloning the arena entry. The arena borrow is released before any
    /// recursive call so the interleaving interner and constant reader
    /// never hold overlapping borrows.
    pub(in crate::analysis) fn const_value(&self, id: ValueId) -> ConstValue {
        let action = {
            let values = self.values.borrow();
            let Some(value) = values.get(id) else {
                return ConstValue::Unknown;
            };
            match value {
                Value::Binding { target, .. } => ConstAction::Binding(*target),
                Value::StaticString(value) => ConstAction::String(value.clone()),
                Value::StaticNumber(value) => ConstAction::Number(*value),
                Value::StaticArray(values) => ConstAction::Array(values.clone()),
                Value::StaticObject(values) => ConstAction::Object(values.clone()),
                _ => ConstAction::Unknown,
            }
        };
        match action {
            ConstAction::Binding(target) => self.const_value(target),
            ConstAction::String(value) => ConstValue::String(value),
            ConstAction::Number(value) => ConstValue::NonNegativeInteger(value),
            ConstAction::Array(ids) => {
                ConstValue::Array(ids.into_iter().map(|id| self.const_value(id)).collect())
            }
            ConstAction::Object(entries) => {
                let mut object = BTreeMap::new();
                for (name_id, value_id) in entries {
                    let Some(key) = self
                        .names
                        .with_mut(|names| names.resolve(name_id).map(SmolStr::new))
                    else {
                        return ConstValue::Unknown;
                    };
                    object.insert(key, self.const_value(value_id));
                }
                ConstValue::Object(object)
            }
            ConstAction::Unknown => ConstValue::Unknown,
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
                let mut arena = self.values.borrow_mut();
                let id = self
                    .names
                    .with_mut(|names| arena.intern_static_object(values, names));
                return binding.map_or(id, |key| arena.intern(Value::Binding { key, target: id }));
            }
        };
        self.values.borrow_mut().intern_with_binding(value, binding)
    }
}
