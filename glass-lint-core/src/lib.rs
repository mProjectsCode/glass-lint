//! Generic, provenance-aware JavaScript linting.
//!
//! Identity policy is intentionally split by ownership. Public rule values
//! such as [`rules::ArgumentIndex`] describe authored semantics and remain
//! stable across compilation. Compiler, classification,
//! cache, and linker IDs are phase-local and crate-private; they must not be
//! compared across artifacts. [`project::ProjectRelativePath`] is the
//! validated project boundary identity, while parser diagnostics retain their
//! authored filename for standalone parser use.

//! Core owns provider-neutral parsing, semantic analysis, matcher execution,
//! bounded reports, and diagnostics. Host policy and rule catalogs are passed
//! in through explicit configuration rather than embedded in this crate.

#[doc(hidden)]
#[macro_export]
macro_rules! impl_test_id_constructor {
    ($id:ident, $raw:ty) => {
        impl $id {
            pub(in $crate::analysis) const fn from_test(raw: $raw) -> Self {
                Self(raw)
            }
        }
    };
}

mod analysis;
mod api;
mod config;
mod diagnostic;
mod ecma_version;
mod environment;
mod limits;
mod lint;
mod parse;
pub mod project;
mod rule_id;

pub use api::rule::{Rule, RuleBuildError};
pub use config::CoreConfig;
pub use diagnostic::{RuleMetadata, Severity, SourceLineIndex};
pub use ecma_version::{
    EcmaFeature, EcmaVersion, EcmaVersionReport, analyze_ecma_version,
    analyze_ecma_version_with_limits,
};
pub use environment::{Environment, EnvironmentError};
pub use limits::{
    AnalysisLimitError, AnalysisLimits, ProjectAdmissionLimitError, ProjectAdmissionLimits,
};
pub use lint::{
    BatchOptions, BatchResult, BatchResults, BatchStartError, LintConfigError, Linter,
    LinterConfig, PreparedRuleSelection, ProjectAnalysis, ProjectAnalysisTimings,
    ProviderCatalogError, RuleBaseline, RuleCatalog, RuleCompilationError, RuleOverride,
    RuleSelection, RuleState,
};
pub use parse::{ParseDiagnostic, ParseFailureKind, SourceLanguage};
pub use project::MatchCertainty;
pub use rule_id::RuleId;

/// Public rule-authoring and matcher types.
pub mod rules {
    pub use crate::api::rule::{
        ArgumentMatcher, CatalogRuleBuilder, Confidence, EventQuery, EventRequirement,
        IntoLifecycleCompletion, IntoLifecycleCondition, IntoLifecycleEvent, IntoLifecycleQuery,
        IntoLifecycleSink, IntoLifecycleSource, IntoQueryDecl, LifecycleCompletion,
        LifecycleCondition, LifecycleEvent, LifecycleQuery, LifecycleSink, MatchKind,
        QueryBuildError, QueryDecl, Rule, RuleBuildError, RuleBuilder, Severity, ValueMatcher,
    };
}

#[cfg(test)]
pub(crate) fn parse_test_source(
    source: &str,
    filename: &str,
) -> Result<parse::ParsedSource, ParseDiagnostic> {
    let source =
        project::SourceFile::with_language(filename, source, parse::SourceLanguage::JavaScript)
            .expect("test parser inputs should have valid relative paths");
    parse::SourceParser::new(&source)?.parse()
}

/// Version of the serialized analysis-report schema.
pub const REPORT_VERSION: u32 = 6;
/// Maximum source size accepted by core, in bytes.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
