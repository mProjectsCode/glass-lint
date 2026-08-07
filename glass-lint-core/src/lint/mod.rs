//! Rule catalog selection, semantic execution, and finding assembly.
//!
//! Linting parses/analyzes once, projects selected matcher plans, then converts
//! located evidence into deterministic findings. Range policy and catalog
//! policy stay separate from semantic fact construction.

mod batch;
mod catalog;
mod linter;
mod ranges;
mod report;
mod selection;

pub use batch::{BatchOptions, BatchResult, BatchResults, BatchStartError};
pub use catalog::{ProviderCatalogError, RuleCatalog};
pub use linter::{Linter, LinterConfig};
pub use report::ProjectAnalysis;
pub use selection::{LintConfigError, RuleBaseline, RuleOverride, RuleSelection, RuleState};

// This bridge is visible to the crate root so sibling phase modules can use
// the private assembler without making the lint module part of the public API.
#[allow(clippy::redundant_pub_crate)]
pub(super) fn finish_report(
    catalog: &RuleCatalog,
    enabled: &[crate::api::classification::RuleIndex],
    evidence_limit: usize,
    sources: &crate::project::SourceTable,
    link_input: crate::analysis::ResolvedLinkInput,
    parse_diagnostics: std::collections::BTreeMap<
        crate::project::ProjectRelativePath,
        crate::ParseDiagnostic,
    >,
    limits: &crate::AnalysisLimits,
) -> ProjectAnalysis {
    report::ReportAssembly::new(catalog, enabled, evidence_limit).finish(
        sources,
        link_input,
        parse_diagnostics,
        limits,
    )
}
