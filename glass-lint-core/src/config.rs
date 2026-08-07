//! Provider-neutral rule selection configuration.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{AnalysisLimits, RuleOverride};

/// Provider-neutral choices that affect analysis, independent of files or
/// presentation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct CoreConfig {
    /// Ordered overrides applied to the CLI-selected rule baseline.
    #[cfg_attr(feature = "serde", serde(default))]
    pub overrides: Vec<RuleOverride>,
    /// Parser and semantic operation bounds for cost-controlled analysis.
    #[cfg_attr(feature = "serde", serde(default))]
    pub limits: AnalysisLimits,
}
