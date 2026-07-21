//! Rule selection types for linter configuration.
//!
//! A [`RuleSelection`] combines a baseline policy with per-rule overrides
//! that enable or disable rules by pattern. Selectors support `*` wildcards
//! for matching groups of rules.

use crate::{RuleId, lint::catalog::RuleCatalog};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleBaseline {
    #[default]
    All,
    None,
    MinimumConfidence(crate::api::rule::Confidence),
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuleState {
    Disabled,
    Enabled,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleOverride {
    #[serde(deserialize_with = "deserialize_selector")]
    selector: RuleSelector,
    enabled: bool,
}

/// Parsed rule selector. The wildcard language is intentionally tiny: `*`
/// matches any sequence of characters, while all other characters are
/// literal. Keeping the parsed shape here prevents validation and execution
/// from maintaining separate interpretations of the same selector.
#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct RuleSelector(String);

fn deserialize_selector<'de, D>(deserializer: D) -> Result<RuleSelector, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    RuleSelector::parse(value).map_err(serde::de::Error::custom)
}

impl RuleSelector {
    fn parse(selector: String) -> Result<Self, LintConfigError> {
        if selector.is_empty()
            || selector
                .chars()
                .any(|c| c == '?' || c == '[' || c == ']' || c == '{' || c == '}')
        {
            return Err(LintConfigError::InvalidSelector(selector));
        }
        RuleId::parse(selector.replace('*', "placeholder"))
            .map_err(|_| LintConfigError::InvalidSelector(selector.clone()))?;
        Ok(Self(selector))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn matches(&self, id: &str) -> bool {
        self.0.split('*').enumerate().all(|(index, part)| {
            if index == 0 {
                id.starts_with(part)
            } else {
                id.contains(part)
            }
        }) && (self.0.ends_with('*') || id.ends_with(self.0.rsplit('*').next().unwrap_or_default()))
    }
}

impl RuleOverride {
    pub fn new(selector: impl Into<String>, state: RuleState) -> Result<Self, LintConfigError> {
        let selector = RuleSelector::parse(selector.into())?;
        Ok(Self {
            selector,
            enabled: state == RuleState::Enabled,
        })
    }

    pub fn selector(&self) -> &str {
        self.selector.as_str()
    }

    pub fn state(&self) -> RuleState {
        if self.enabled {
            RuleState::Enabled
        } else {
            RuleState::Disabled
        }
    }

    pub(in crate::lint) fn matches(&self, id: &str) -> bool {
        self.selector.matches(id)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleSelection {
    baseline: RuleBaseline,
    overrides: Vec<RuleOverride>,
}

impl Default for RuleSelection {
    fn default() -> Self {
        Self::new(RuleBaseline::All)
    }
}

impl RuleSelection {
    pub fn new(baseline: RuleBaseline) -> Self {
        Self {
            baseline,
            overrides: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_override(mut self, value: RuleOverride) -> Self {
        self.overrides.push(value);
        self
    }

    pub fn baseline(&self) -> RuleBaseline {
        self.baseline
    }

    pub fn overrides(&self) -> &[RuleOverride] {
        &self.overrides
    }

    pub fn validate_against(&self, catalog: &RuleCatalog) -> Result<(), LintConfigError> {
        for override_ in self.overrides() {
            if catalog
                .rule_ids()
                .iter()
                .any(|id| override_.selector.matches(id.as_str()))
            {
                continue;
            }
            if !override_.selector.as_str().contains('*') {
                return Err(LintConfigError::UnknownRule(
                    RuleId::parse(override_.selector.as_str().to_owned()).map_err(|_| {
                        LintConfigError::InvalidSelector(override_.selector.as_str().into())
                    })?,
                ));
            }
            return Err(LintConfigError::InvalidSelector(
                override_.selector.as_str().into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Configuration failure when selecting rules for a linter.
pub enum LintConfigError {
    /// A requested fully-qualified rule ID is absent from the catalog.
    UnknownRule(RuleId),
    /// A selector is malformed or did not select any assembled rule.
    InvalidSelector(String),
    /// A catalog contains the same fully-qualified rule more than once.
    DuplicateRule(RuleId),
    /// Safety limits are invalid.
    InvalidLimits(String),
}

impl std::fmt::Display for LintConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(f, "unknown rule `{id}`"),
            Self::InvalidSelector(message) => write!(f, "invalid rule selector: {message}"),
            Self::DuplicateRule(id) => write!(f, "duplicate rule `{id}`"),
            Self::InvalidLimits(message) => write!(f, "invalid resource limits: {message}"),
        }
    }
}

impl std::error::Error for LintConfigError {}
