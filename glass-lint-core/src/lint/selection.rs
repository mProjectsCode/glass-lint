//! Rule selection types for linter configuration.
//!
//! A [`RuleSelection`] combines a baseline policy with per-rule overrides
//! that enable or disable rules by pattern. Selectors support `*` wildcards
//! for matching groups of rules.

use crate::{
    RuleId,
    api::classification::RuleIndex,
    lint::catalog::{RuleCatalog, RuleCompilationError},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RuleBaseline {
    #[default]
    All,
    None,
    MinimumConfidence(crate::api::rule::Confidence),
}

impl RuleBaseline {
    /// Baseline used by the recommended built-in profile.
    pub const fn recommended() -> Self {
        Self::MinimumConfidence(crate::api::rule::Confidence::High)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum RuleState {
    Disabled,
    Enabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RuleOverride {
    #[cfg_attr(feature = "serde", serde(deserialize_with = "deserialize_selector"))]
    selector: RuleSelector,
    #[cfg_attr(
        feature = "serde",
        serde(rename = "enabled", with = "rule_state_as_bool")
    )]
    state: RuleState,
}

/// A segment of a parsed rule pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
enum PatternSegment {
    /// A literal string that must appear verbatim.
    Literal(String),
    /// A `*` wildcard matching any sequence of characters.
    Wildcard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RulePattern {
    segments: Vec<PatternSegment>,
    ends_with_wildcard: bool,
}

impl RulePattern {
    fn parse(selector: &str) -> Result<Self, LintConfigError> {
        let Some((provider, name)) = selector.split_once(':') else {
            return Err(LintConfigError::InvalidSelector(selector.to_owned()));
        };
        if selector[provider.len() + 1..].contains(':')
            || !valid_pattern_part(provider, false)
            || !valid_pattern_part(name, true)
        {
            return Err(LintConfigError::InvalidSelector(selector.to_owned()));
        }

        let mut segments = Vec::new();
        for part in selector.split('*') {
            if !part.is_empty() {
                segments.push(PatternSegment::Literal(part.to_owned()));
            }
            segments.push(PatternSegment::Wildcard);
        }
        segments.pop();
        Ok(Self {
            segments,
            ends_with_wildcard: selector.ends_with('*'),
        })
    }

    fn has_wildcard(&self) -> bool {
        self.ends_with_wildcard
            || self
                .segments
                .iter()
                .any(|segment| matches!(segment, PatternSegment::Wildcard))
    }

    fn matches(&self, id: &str) -> bool {
        let mut pos = 0usize;
        for (index, segment) in self.segments.iter().enumerate() {
            let PatternSegment::Literal(literal) = segment else {
                continue;
            };
            if index == 0 {
                if !id.starts_with(literal) {
                    return false;
                }
                pos = literal.len();
            } else if index == self.segments.len() - 1 && !self.ends_with_wildcard {
                if !id[pos..].ends_with(literal) {
                    return false;
                }
            } else {
                let Some(found) = id[pos..].find(literal) else {
                    return false;
                };
                pos += found + literal.len();
            }
        }
        true
    }
}

fn valid_pattern_part(part: &str, allow_dot: bool) -> bool {
    if part.is_empty() || part.starts_with(['-', '_', '.']) || part.contains("..") {
        return false;
    }
    part.chars().enumerate().all(|(index, character)| {
        character == '*'
            || character.is_ascii_lowercase()
            || (index > 0 && character.is_ascii_digit())
            || character == '-'
            || character == '_'
            || (allow_dot && character == '.')
    }) && !part.ends_with(['-', '_', '.'])
}

/// Parsed rule selector. The wildcard language is intentionally tiny: `*`
/// matches any sequence of characters, while all other characters are
/// literal. Keeping the parsed shape here prevents validation and execution
/// from maintaining separate interpretations of the same selector.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RuleSelector {
    /// Original selector text for serialization and display.
    raw: String,
    /// Validated wildcard pattern used for O(n) matching.
    pattern: RulePattern,
}

#[cfg(feature = "serde")]
fn deserialize_selector<'de, D>(deserializer: D) -> Result<RuleSelector, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    RuleSelector::parse(value).map_err(serde::de::Error::custom)
}

#[cfg(feature = "serde")]
mod rule_state_as_bool {
    use super::RuleState;

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn serialize<S>(state: &RuleState, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bool(*state == RuleState::Enabled)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<RuleState, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(
            if <bool as serde::Deserialize>::deserialize(deserializer)? {
                RuleState::Enabled
            } else {
                RuleState::Disabled
            },
        )
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for RuleSelector {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
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
        let pattern = RulePattern::parse(&selector)?;
        if !pattern.has_wildcard() {
            RuleId::parse(selector.clone())
                .map_err(|_| LintConfigError::InvalidSelector(selector.clone()))?;
        }

        Ok(Self {
            raw: selector,
            pattern,
        })
    }

    fn as_str(&self) -> &str {
        &self.raw
    }

    fn has_wildcard(&self) -> bool {
        self.pattern.has_wildcard()
    }

    fn matches(&self, id: &str) -> bool {
        // No wildcard: exact match.
        if !self.has_wildcard() {
            return id == self.raw;
        }
        self.pattern.matches(id)
    }
}

impl RuleOverride {
    pub fn new(selector: impl Into<String>, state: RuleState) -> Result<Self, LintConfigError> {
        let selector = RuleSelector::parse(selector.into())?;
        Ok(Self { selector, state })
    }

    pub fn selector(&self) -> &str {
        self.selector.as_str()
    }

    pub fn state(&self) -> RuleState {
        self.state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RuleSelection {
    baseline: RuleBaseline,
    overrides: Vec<RuleOverride>,
}

struct SelectionEvaluation {
    matched_overrides: Vec<bool>,
    enabled: Vec<RuleIndex>,
}

/// A catalog-bound, validated rule selection ready for linter construction.
///
/// The prepared value owns the exact combined catalog used during validation,
/// keeping its enabled indexes aligned with the catalog order without asking
/// linter construction to evaluate the selection again.
#[derive(Clone, Debug)]
pub struct PreparedRuleSelection {
    catalog: RuleCatalog,
    enabled: Vec<RuleIndex>,
}

impl PreparedRuleSelection {
    pub(crate) fn into_parts(self) -> (RuleCatalog, Vec<RuleIndex>) {
        (self.catalog, self.enabled)
    }
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

    /// Validate every override against an assembled catalog.
    ///
    /// Resolve and validate this selection against an assembled catalog.
    pub fn prepare(&self, catalog: &RuleCatalog) -> Result<PreparedRuleSelection, LintConfigError> {
        let evaluation = self.validated_evaluation(catalog)?;
        Ok(PreparedRuleSelection {
            catalog: catalog.clone(),
            enabled: evaluation.enabled,
        })
    }

    pub(crate) fn resolve(&self, catalog: &RuleCatalog) -> Result<Vec<RuleIndex>, LintConfigError> {
        Ok(self.validated_evaluation(catalog)?.enabled)
    }

    fn validated_evaluation(
        &self,
        catalog: &RuleCatalog,
    ) -> Result<SelectionEvaluation, LintConfigError> {
        let evaluation = self.evaluate(catalog);
        self.validate_override_matches(&evaluation.matched_overrides)?;
        Ok(evaluation)
    }

    fn evaluate(&self, catalog: &RuleCatalog) -> SelectionEvaluation {
        let mut matched_overrides = vec![false; self.overrides.len()];
        let mut enabled = Vec::new();

        for (index, record) in catalog.compiled().iter().enumerate() {
            let rule_id = &record.rule_id;
            let baseline = match self.baseline {
                RuleBaseline::All => true,
                RuleBaseline::None => false,
                RuleBaseline::MinimumConfidence(confidence) => record.confidence.meets(confidence),
            };
            let mut state = baseline;
            for (override_index, override_) in self.overrides.iter().enumerate() {
                if override_.selector.matches(rule_id.as_str()) {
                    matched_overrides[override_index] = true;
                    state = override_.state() == RuleState::Enabled;
                }
            }
            if state {
                enabled.push(RuleIndex::new(index));
            }
        }

        SelectionEvaluation {
            matched_overrides,
            enabled,
        }
    }

    fn validate_override_matches(&self, matched_overrides: &[bool]) -> Result<(), LintConfigError> {
        for (override_, matched) in self.overrides.iter().zip(matched_overrides) {
            if *matched {
                continue;
            }
            if !override_.selector.has_wildcard() {
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
    ///
    /// Catalog composition runs during linter construction, so its failure is
    /// re-hosted here to keep one error type for the whole construction
    /// surface.
    DuplicateRule(RuleId),
    /// A catalog rule failed validation or matcher/query compilation.
    InvalidRule(RuleId, RuleCompilationError),
}

impl std::fmt::Display for LintConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRule(id) => write!(f, "unknown rule `{id}`"),
            Self::InvalidSelector(message) => write!(f, "invalid rule selector: {message}"),
            Self::DuplicateRule(id) => write!(f, "duplicate rule `{id}`"),
            Self::InvalidRule(id, message) => write!(f, "invalid rule `{id}`: {message}"),
        }
    }
}

impl std::error::Error for LintConfigError {}

#[cfg(test)]
mod tests;
