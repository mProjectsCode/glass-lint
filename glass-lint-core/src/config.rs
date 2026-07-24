//! Provider-neutral rule selection configuration.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    AnalysisLimits, RuleCatalog,
    lint::{LintConfigError, RuleSelection},
};

/// Provider-neutral choices that affect analysis, independent of files or
/// presentation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CoreConfig {
    /// Baseline and ordered overrides for the assembled provider catalogs.
    #[cfg_attr(feature = "serde", serde(default))]
    pub selection: RuleSelection,
    /// Parser and semantic operation bounds for cost-controlled analysis.
    #[cfg_attr(feature = "serde", serde(default))]
    pub limits: AnalysisLimits,
}

impl CoreConfig {
    /// Validate the selection against a concrete catalog.
    /// Limits are guaranteed valid by construction through
    /// [`AnalysisLimits::new`] or deserialization and do not need
    /// re-validation.
    pub fn validate(&self, catalog: &RuleCatalog) -> Result<(), LintConfigError> {
        self.selection.validate_against(catalog)
    }
}
