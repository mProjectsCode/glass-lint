//! Conservative conversion between bounded constants and static scope facts.

use glass_lint_datastructures::NameId;
use smol_str::SmolStr;

use crate::analysis::{
    model::{StaticProperties, scope::BindingProvenance},
    syntax::constant::ConstValue,
};

/// Convert the constant subset that can be retained as lexical provenance.
///
/// Object values are intentionally retained as keys only: unknown constant
/// members do not establish a value witness. Name admission is delegated to
/// the caller so this adapter cannot bypass the active scope budget.
pub(in crate::analysis) fn const_value_to_provenance(
    value: ConstValue,
    intern_name: &mut impl FnMut(&str) -> Option<NameId>,
) -> Option<BindingProvenance> {
    match value {
        ConstValue::String(value) => Some(BindingProvenance::StaticString(value)),
        ConstValue::NonNegativeInteger(value) => Some(BindingProvenance::StaticNumber(value)),
        ConstValue::Array(values) => Some(BindingProvenance::StaticStringArray(
            values
                .into_iter()
                .map(|value| value.string().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?,
        )),
        ConstValue::Object(values) => {
            let mut keys = StaticProperties::new();
            for key in values.keys() {
                let name = intern_name(key)?;
                if !keys.insert(name, ()) {
                    return None;
                }
            }
            Some(BindingProvenance::StaticObjectKeys(keys))
        }
        ConstValue::Unknown => None,
    }
}

/// Convert retained static provenance back into the bounded constant view.
pub(in crate::analysis) fn provenance_to_const_value(
    provenance: &BindingProvenance,
    resolve_name: &impl Fn(NameId) -> Option<SmolStr>,
) -> ConstValue {
    match provenance {
        BindingProvenance::StaticString(value) => ConstValue::String(value.clone()),
        BindingProvenance::StaticNumber(value) => ConstValue::NonNegativeInteger(*value),
        BindingProvenance::StaticStringArray(values) => {
            ConstValue::array(values.iter().cloned().map(ConstValue::String).collect()).bounded()
        }
        BindingProvenance::StaticObjectKeys(values) => values
            .to_const_object(resolve_name)
            .map_or(ConstValue::Unknown, ConstValue::bounded),
        BindingProvenance::StaticObjectValues(values) => values
            .to_const_object(resolve_name)
            .map_or(ConstValue::Unknown, ConstValue::bounded),
        _ => ConstValue::Unknown,
    }
}
