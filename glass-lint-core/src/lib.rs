//! Generic, provenance-aware JavaScript linting.

//! Core owns provider-neutral parsing, semantic analysis, matcher execution,
//! bounded reports, and diagnostics. Host policy and rule catalogs are passed
//! in through explicit configuration rather than embedded in this crate.

mod analysis;
mod api;
mod config;
mod diagnostic;
mod environment;
mod limits;
mod lint;
mod parse;
pub mod project;
mod rule_id;

pub use api::rule::{Rule, RuleBuildError};
pub use config::CoreConfig;
pub use diagnostic::{RuleMetadata, Severity, SourceLineIndex};
pub use environment::{Environment, EnvironmentError};
pub use limits::{AnalysisLimitError, AnalysisLimits};
pub use lint::{
    LintConfigError, Linter, LinterConfig, ProjectAnalysis, ProviderCatalogError, RuleBaseline,
    RuleCatalog, RuleOverride, RuleSelection, RuleState,
};
pub use parse::{ParseDiagnostic, SourceLanguage};
pub use project::MatchCertainty;
pub use rule_id::RuleId;
/// Public rule-authoring and matcher types.
pub mod rules {
    pub use crate::api::{
        classification::MatchKind,
        rule::{
            ArgumentMatcher, Category, Confidence, EventQuery, EventRequirement, EventSpec,
            IdentitySpec, IntoLifecycleSource, IntoQueryDecl, LifecycleCompletion,
            LifecycleCondition, LifecycleEvent, LifecycleQuery, LifecycleSink, QueryBuildError,
            QueryDecl, Rule, RuleBuildError, RuleBuilder as Builder, Severity, ValueMatcher, VarId,
        },
    };
}

#[cfg(test)]
pub(crate) use parse::parse;

/// Version of the serialized analysis-report schema.
pub const REPORT_VERSION: u32 = 6;
/// Maximum source size accepted by core, in bytes.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
