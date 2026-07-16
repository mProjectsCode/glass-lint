//! Provider-neutral rule selection configuration.

use serde::{Deserialize, Serialize};

use crate::{ResourceLimits, RuleCatalog, RuleId, lint::LintConfigError};

/// Provider-neutral choices that affect analysis, independent of files or
/// presentation.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CoreConfig {
    /// `None` preserves the provider profile; `Some([])` disables all rules.
    #[serde(default)]
    pub rules: Option<Vec<RuleId>>,
    #[serde(default)]
    pub limits: ResourceLimits,
}

impl CoreConfig {
    /// Validate selected rule IDs against a concrete catalog.
    pub fn validate(&self, catalog: &RuleCatalog) -> Result<(), LintConfigError> {
        self.limits
            .validate()
            .map_err(LintConfigError::InvalidLimits)?;
        if let Some(rules) = &self.rules {
            let known = catalog.rule_ids();
            if let Some(rule) = rules.iter().find(|rule| !known.contains(rule)) {
                return Err(LintConfigError::UnknownRule(rule.clone()));
            }
        }
        Ok(())
    }
}
