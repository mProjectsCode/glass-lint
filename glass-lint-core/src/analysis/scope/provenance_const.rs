use std::collections::BTreeMap;

use glass_lint_datastructures::NameId;
use smol_str::SmolStr;

use crate::analysis::{scope::BindingProvenance, syntax::constant::ConstValue};

pub(in crate::analysis) fn provenance_to_const_value(
    provenance: &BindingProvenance,
    resolve_name: &impl Fn(NameId) -> Option<SmolStr>,
) -> ConstValue {
    match provenance {
        BindingProvenance::StaticString(value) => ConstValue::String(value.clone()),
        BindingProvenance::StaticNumber(value) => ConstValue::NonNegativeInteger(*value),
        BindingProvenance::StaticStringArray(values) => {
            ConstValue::Array(values.iter().cloned().map(ConstValue::String).collect())
        }
        BindingProvenance::StaticObjectKeys(values) => ConstValue::Object(
            values
                .iter()
                .filter_map(|key| resolve_name(*key))
                .map(|key| (key, ConstValue::Unknown))
                .collect::<BTreeMap<_, _>>(),
        ),
        BindingProvenance::StaticObjectValues(values) => ConstValue::Object(
            values
                .keys()
                .filter_map(|key| resolve_name(*key))
                .map(|key| (key, ConstValue::Unknown))
                .collect::<BTreeMap<_, _>>(),
        ),
        _ => ConstValue::Unknown,
    }
}
