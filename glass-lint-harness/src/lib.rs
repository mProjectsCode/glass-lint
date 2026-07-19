//! Reusable conformance harness for cases, adapters, reports, and profiling.
//!
//! This crate keeps execution policy independent from the CLI so tests and
//! alternate front ends observe the same normalization and
//! comparison rules.

mod adapters;
mod builtins;
mod cases;
mod profile;
mod profile_manifest;
mod report;
mod runner;
mod types;

pub use adapters::{Adapter, ExternalAdapter, GlassLintAdapter};
pub use cases::load_cases;
pub use profile::{
    ProfileCatalogProvider, ProfileConfig, ProfileConfigBuilder, ProfileCorpusIdentity,
    ProfileOperationCounts, ProfilePhaseTimings, ProfileRepetitionSummary, ProfileSummary,
    ProfileWorkload, ProfileWorkloadIdentity, ProfileWorkloadSummary, RuleSelectionProfile,
    discover_profile_files, ensure_profile_correctness_match, run_profile,
};
pub use profile_manifest::{
    ProfileManifest, ProfileManifestEntry, VerifiedProfileManifest, create_profile_manifest,
    verify_profile_manifest,
};
pub use report::{
    render_adapter_comparison, render_suite_failures, render_suite_markdown, render_suite_summary,
    serialize_analysis_report,
};
pub use runner::{AdapterTimings, run_suite};
pub use types::{
    ADAPTER_PROTOCOL_VERSION, AdapterFile, AdapterProject, AdapterRequest, AdapterResolution,
    AdapterResolutionKind, AdapterResolutionResult, AdapterResponse, AdapterRun, Case, CaseResult,
    ExpectedCount, FindingExpectation, ProjectCase, SuiteReport, ToolExpectation, ToolResult,
};

#[cfg(test)]
mod test_support;
