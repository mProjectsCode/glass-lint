use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RuleId(String);

impl RuleId {
    /// Validate the name portion used by provider-local rule builders.
    ///
    /// The namespaced form adds a provider prefix and `:`, but both forms
    /// share the same canonical character and separator rules for the name.
    pub(crate) fn valid_name(value: &str) -> bool {
        Self::valid_part(value, true)
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, crate::RuleCatalogError> {
        let value = value.into();
        let Some((provider, name)) = value.split_once(':') else {
            return Err(crate::RuleCatalogError::InvalidRuleId(value));
        };
        if !Self::valid_part(provider, false) || !Self::valid_part(name, true) {
            return Err(crate::RuleCatalogError::InvalidRuleId(value));
        }
        Ok(Self(value))
    }

    fn valid_part(part: &str, allow_dot: bool) -> bool {
        !part.is_empty()
            && part.chars().enumerate().all(|(index, character)| {
                (index > 0 && character.is_ascii_digit())
                    || character.is_ascii_lowercase()
                    || character == '-'
                    || character == '_'
                    || (allow_dot && character == '.')
            })
            && !part.starts_with(['-', '_', '.'])
            && !part.ends_with(['-', '_', '.'])
            && !part.contains("..")
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::parse(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl Error for RuleId {}
