//! Provider-neutral rule selection configuration.

use serde::{Deserialize, Serialize};

use crate::{
    AnalysisLimits, RuleCatalog, RuleId,
    lint::{LintConfigError, RuleSelection},
};

/// Provider-neutral choices that affect analysis, independent of files or
/// presentation.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CoreConfig {
    /// Baseline and ordered overrides for the assembled provider catalogs.
    #[serde(default)]
    pub selection: RuleSelection,
    #[serde(default)]
    pub limits: AnalysisLimits,
}

impl CoreConfig {
    /// Validate the selection and limits against a concrete catalog.
    pub fn validate(&self, catalog: &RuleCatalog) -> Result<(), LintConfigError> {
        self.limits
            .validate()
            .map_err(LintConfigError::InvalidLimits)?;
        for override_ in self.selection.overrides() {
            if !catalog
                .rule_ids()
                .iter()
                .any(|id| crate::lint::selector_matches(override_.selector(), id.as_str()))
            {
                if !override_.selector().contains('*') {
                    let rule = RuleId::parse(override_.selector().to_owned()).map_err(|_| {
                        LintConfigError::InvalidSelector(override_.selector().to_owned())
                    })?;
                    return Err(LintConfigError::UnknownRule(rule));
                }
                return Err(LintConfigError::InvalidSelector(
                    override_.selector().to_owned(),
                ));
            }
        }
        Ok(())
    }
}
