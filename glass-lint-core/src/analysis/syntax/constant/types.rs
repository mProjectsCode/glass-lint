use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};

pub(in crate::analysis) const MAX_DEPTH: usize = 32;
pub(super) const MAX_NODES: usize = 4_096;
pub(super) const MAX_LOOKUPS: usize = 512;
pub(super) const MAX_STRING_BYTES: usize = 16 * 1024;
pub(in crate::analysis) const MAX_ARRAY_ITEMS: usize = 256;
/// Maximum number of distinct keys retained in a static object shape.
pub(in crate::analysis) const MAX_OBJECT_KEYS: usize = 256;

/// Merge one static object shape without exceeding the distinct-key bound.
pub(super) fn merge_bounded(
    target: &mut BTreeMap<SmolStr, ConstValue>,
    added: BTreeMap<SmolStr, ConstValue>,
) -> bool {
    if target.len().saturating_add(added.len()) > MAX_OBJECT_KEYS {
        return false;
    }
    target.extend(added);
    true
}

enum ScalarPropertyText<'a> {
    String(&'a str),
    NonNegativeInteger(usize),
}

impl ScalarPropertyText<'_> {
    fn into_smolstr(self) -> SmolStr {
        match self {
            Self::String(value) => value.to_smolstr(),
            Self::NonNegativeInteger(value) => value.to_smolstr(),
        }
    }

    fn into_string(self) -> String {
        match self {
            Self::String(value) => value.to_owned(),
            Self::NonNegativeInteger(value) => value.to_string(),
        }
    }
}

/// Convert a finite, integral, non-negative number into a bounded index type.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]
pub(in crate::analysis) fn non_negative_integer(value: f64) -> Option<usize> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
        return None;
    }
    let n = value as usize;
    if n as f64 != value {
        return None;
    }
    Some(n)
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Finite constant shapes accepted by semantic matching.
pub(in crate::analysis) enum ConstValue {
    /// Evaluation was unsupported, dynamic, or over budget.
    Unknown,
    /// A bounded string literal or concatenation.
    String(String),
    /// A finite non-negative integer usable as a property/index key.
    NonNegativeInteger(usize),
    /// A bounded array whose elements may themselves be unknown.
    Array(Vec<Self>),
    /// A bounded static object shape keyed in deterministic order.
    Object(BTreeMap<SmolStr, Self>),
}

impl ConstValue {
    /// Admit an array only when its container bound is still satisfied.
    pub(in crate::analysis) fn array(values: Vec<Self>) -> Self {
        if values.len() <= MAX_ARRAY_ITEMS {
            Self::Array(values)
        } else {
            Self::Unknown
        }
    }

    /// Admit an object only when its distinct-key bound is still satisfied.
    pub(in crate::analysis) fn object(values: BTreeMap<SmolStr, Self>) -> Self {
        if values.len() <= MAX_OBJECT_KEYS {
            Self::Object(values)
        } else {
            Self::Unknown
        }
    }

    /// Re-apply the complete constant-domain bounds to a materialized tree.
    ///
    /// Syntax evaluation charges its own expression budget while arena and
    /// provenance projections start from already-materialized values. Keeping
    /// this final admission policy here prevents those projections from
    /// creating larger or deeper trees than the evaluator can accept.
    pub(in crate::analysis) fn bounded(self) -> Self {
        fn visit(value: ConstValue, depth: usize, nodes: &mut usize) -> ConstValue {
            if depth >= MAX_DEPTH || *nodes >= MAX_NODES {
                return ConstValue::Unknown;
            }
            *nodes += 1;
            match value {
                ConstValue::String(value) => ConstValue::bounded_string(value),
                ConstValue::NonNegativeInteger(value) => ConstValue::NonNegativeInteger(value),
                ConstValue::Array(values) if values.len() <= MAX_ARRAY_ITEMS => ConstValue::array(
                    values
                        .into_iter()
                        .map(|value| visit(value, depth + 1, nodes))
                        .collect(),
                ),
                ConstValue::Object(values) if values.len() <= MAX_OBJECT_KEYS => {
                    ConstValue::object(
                        values
                            .into_iter()
                            .map(|(key, value)| (key, visit(value, depth + 1, nodes)))
                            .collect(),
                    )
                }
                ConstValue::Unknown | ConstValue::Array(_) | ConstValue::Object(_) => {
                    ConstValue::Unknown
                }
            }
        }

        let mut nodes = 0;
        visit(self, 0, &mut nodes)
    }

    /// Construct a string only when it fits the evaluator's global bound.
    /// Keeping the limit at the value boundary prevents one evaluation path
    /// from accidentally returning an oversized string.
    pub(super) fn bounded_string(value: String) -> Self {
        if value.len() <= MAX_STRING_BYTES {
            Self::String(value)
        } else {
            Self::Unknown
        }
    }

    /// Borrow the value when this is a string constant.
    pub(in crate::analysis) fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Convert string/integer constants into static property keys.
    pub(in crate::analysis) fn property_key(&self) -> Option<SmolStr> {
        self.scalar_property_text()
            .map(ScalarPropertyText::into_smolstr)
    }

    /// Return deterministic keys when this is a static object.
    #[cfg(test)]
    pub(in crate::analysis) fn object_keys(&self) -> Option<Vec<SmolStr>> {
        match self {
            Self::Object(values) => Some(values.keys().cloned().collect()),
            _ => None,
        }
    }

    pub(super) fn to_property_string(&self) -> Option<String> {
        self.scalar_property_text()
            .map(ScalarPropertyText::into_string)
    }

    fn scalar_property_text(&self) -> Option<ScalarPropertyText<'_>> {
        match self {
            Self::String(value) => Some(ScalarPropertyText::String(value)),
            Self::NonNegativeInteger(value) => Some(ScalarPropertyText::NonNegativeInteger(*value)),
            _ => None,
        }
    }
}
