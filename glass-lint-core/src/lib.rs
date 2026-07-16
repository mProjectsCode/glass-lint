//! Generic, provenance-aware JavaScript linting.
//!
//! Core owns provider-neutral parsing, semantic analysis, matcher execution,
//! bounded reports, and diagnostics. Host policy and rule catalogs are passed
//! in through explicit configuration rather than embedded in this crate.

pub mod analysis;
pub mod api;
pub mod budget;
mod config;
mod diagnostic;
mod environment;
pub mod lint;
pub mod parse;
pub mod project;
pub mod report;
mod rule_id;
#[cfg(feature = "telemetry")]
pub mod telemetry;

pub use api::rule::{Rule, RuleBuildError as BuildError};
pub use config::CoreConfig;
pub use diagnostic::{
    Evidence, Finding, LintReport, Position, RuleMetadata, Severity, SourceRange,
};
pub use environment::{Environment, EnvironmentError};
pub use lint::{LintConfigError, Linter, RuleCatalog, RuleCatalogError};
pub use parse::{ParseDiagnostic, SourceLanguage};
pub use project::{
    EvidenceList, ProjectDiagnostic, ProjectEvidence, ProjectFileReport, ProjectFinding,
    ProjectInput, ProjectInputError, ProjectOperationCounts, ProjectReport, ProjectReportSummary,
    ProjectSession, ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind,
    ResolutionResult, SourceFile, SourceLocation,
};
pub use report::{PrettyFile, PrettyOptions, PrettyReport, PrettyReports};
pub use rule_id::RuleId;

pub const REPORT_VERSION: u32 = 3;
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Declarative rule-building API for provider crates and custom catalogs.
pub mod rules {
    pub use crate::api::rule::{
        ArgumentMatcher, CallMatcher, Category, ClassMatcher, Confidence, ConstructorMatcher,
        FlowCompletion, FlowCondition, FlowSinkMatcher, InstanceMemberCallMatcher, Matcher,
        MemberCallMatcher, MemberReadMatcher, ObjectEventMatcher, ObjectFlowMatcher,
        ObjectSourceMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher, Rule,
        RuleBuildError as BuildError, RuleBuilder as Builder, Severity, ValueMatcher,
    };
}
#[cfg(test)]
pub use parse::parse;
