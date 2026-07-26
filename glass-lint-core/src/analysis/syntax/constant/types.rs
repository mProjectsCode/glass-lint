use std::collections::BTreeMap;

use smol_str::{SmolStr, ToSmolStr};

pub(super) const MAX_DEPTH: usize = 32;
pub(super) const MAX_NODES: usize = 4_096;
pub(super) const MAX_LOOKUPS: usize = 512;
pub(super) const MAX_STRING_BYTES: usize = 16 * 1024;
pub(super) const MAX_ARRAY_ITEMS: usize = 256;
pub(super) const MAX_OBJECT_KEYS: usize = 256;

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
        match self {
            Self::String(value) => Some(value.to_smolstr()),
            Self::NonNegativeInteger(value) => Some(value.to_smolstr()),
            _ => None,
        }
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
        match self {
            Self::String(value) => Some(value.clone()),
            Self::NonNegativeInteger(value) => Some(value.to_string()),
            _ => None,
        }
    }
}
