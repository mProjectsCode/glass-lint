//! Generic, provenance-aware JavaScript linting.

mod analysis;
mod api;
mod diagnostic;
mod lint;
mod parse;
mod rule_id;

pub use api::rule::{ApiRule as Rule, ApiRuleBuildError as BuildError};
pub use diagnostic::{
    Evidence, Finding, LintReport, Position, RuleMetadata, Severity, SourceRange,
};
pub use lint::{LintConfigError, Linter, RuleCatalog, RuleCatalogError};
pub use parse::ParseDiagnostic;
#[allow(unused_imports)]
pub(crate) use parse::parse;
pub use rule_id::RuleId;

pub const REPORT_VERSION: u32 = 2;
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Declarative rule-building API for provider crates and custom catalogs.
pub mod rules {
    pub use crate::api::rule::{
        ApiCategory as Category, ApiRule as Rule, ApiRuleBuildError as BuildError,
        ApiRuleBuilder as Builder, ApiSeverity as Severity, CallMatcher, ClassMatcher, Confidence,
        ConstructorMatcher, FlowMatcher, FlowValueMatcher, InstanceMemberCallMatcher, Matcher,
        MemberCallMatcher, MemberReadMatcher, ReturnedMemberCallMatcher, ReturnedMemberReadMatcher,
    };
}
