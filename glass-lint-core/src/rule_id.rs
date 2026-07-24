//! Validated namespaced rule IDs.

use std::{error::Error, fmt};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
/// Canonical `provider:name` rule identifier.
pub struct RuleId(String);

impl RuleId {
    /// Validate the name portion used by provider-local rule builders.
    ///
    /// The namespaced form adds a provider prefix and `:`, but both forms
    /// share the same canonical character and separator rules for the name.
    pub fn valid_name(value: &str) -> bool {
        Self::valid_part(value, true)
    }

    /// Parse and validate a namespaced rule ID.
    pub fn parse(value: impl Into<String>) -> Result<Self, crate::ProviderCatalogError> {
        let value = value.into();
        let Some((provider, name)) = value.split_once(':') else {
            return Err(crate::ProviderCatalogError::InvalidRuleId(value));
        };
        if !Self::valid_part(provider, false) || !Self::valid_part(name, true) {
            return Err(crate::ProviderCatalogError::InvalidRuleId(value));
        }
        Ok(Self(value))
    }

    /// Validate one segment of a namespaced rule ID.
    ///
    /// A valid part contains only lowercase ASCII letters, digits, hyphens,
    /// and underscores. When `allow_dot` is true (for the name portion),
    /// periods are also permitted to support hierarchical names. Leading and
    /// trailing separators are rejected; consecutive dots are rejected.
    fn valid_part(part: &str, allow_dot: bool) -> bool {
        !part.is_empty() && Self::valid_characters(part, allow_dot) && Self::valid_boundaries(part)
    }

    fn valid_characters(part: &str, allow_dot: bool) -> bool {
        part.chars().enumerate().all(|(index, character)| {
            (index > 0 && character.is_ascii_digit())
                || character.is_ascii_lowercase()
                || character == '-'
                || character == '_'
                || (allow_dot && character == '.')
        })
    }

    fn valid_boundaries(part: &str) -> bool {
        !part.starts_with(['-', '_', '.'])
            && !part.ends_with(['-', '_', '.'])
            && !part.contains("..")
    }

    #[must_use]
    /// Borrow the canonical ID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl Error for RuleId {}
