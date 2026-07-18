//! Provider-neutral rule selection configuration.

use serde::{Deserialize, Serialize};

use crate::{
    AnalysisLimits, RuleCatalog,
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
        crate::lint::validate_selection(&self.selection, catalog)
    }
}
