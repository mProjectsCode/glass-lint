//! Generic, provenance-aware JavaScript linting.
//!
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
mod report;
mod rule_id;
#[cfg(feature = "telemetry")]
pub mod telemetry;

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
pub use report::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports, visible_text};
pub use rule_id::RuleId;
pub mod rules {
    pub use crate::api::rule::{
        ArgumentMatcher, Category, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher,
        MatcherDecl, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        RuleBuildError, RuleBuilder as Builder, Severity, ValueMatcher,
    };
}

#[cfg(test)]
pub(crate) use parse::parse;

pub const REPORT_VERSION: u32 = 5;
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
