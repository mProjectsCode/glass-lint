//! Reusable conformance harness for cases, adapters, reports, and profiling.
//!
//! This crate keeps execution policy independent from the CLI so tests and
//! alternate front ends observe the same normalization and
//! comparison rules.

mod adapters;
mod builtins;
mod bundler;
mod cases;
mod profile;
mod profile_manifest;
mod report;
mod runner;
mod types;

pub use adapters::{Adapter, ExternalAdapter, GlassLintAdapter};
pub use bundler::{
    BUNDLER_PROTOCOL_VERSION, BundleOutput, BundleRequest, BundleResponse, Bundler, ProcessBundler,
    digest as bundle_digest, request_for_case,
};
pub use cases::load_cases;
pub use profile::{
    ProfileAnalysisLimits, ProfileCatalogProvider, ProfileConfig, ProfileConfigBuilder,
    ProfileCorpusIdentity, ProfileExecutionIdentity, ProfileOperationCounts, ProfilePhaseTimings,
    ProfileProjectLoadIdentity, ProfileRepetitionSummary, ProfileSummary, ProfileWorkload,
    ProfileWorkloadIdentity, ProfileWorkloadSummary, RuleSelectionProfile, discover_profile_files,
    ensure_profile_correctness_match, run_profile,
};
pub use profile_manifest::{
    ProfileManifest, ProfileManifestEntry, VerifiedProfileManifest, create_profile_manifest,
    verify_profile_manifest,
};
pub use report::{
    render_adapter_comparison, render_suite_failures, render_suite_markdown, render_suite_summary,
    serialize_analysis_report,
};
pub use runner::{
    AdapterTimings, BundleTimings, compare_rule_counts, run_suite, run_suite_with_bundler,
};
pub use types::{
    ADAPTER_PROTOCOL_VERSION, AdapterConversionError, AdapterFile, AdapterProject, AdapterRequest,
    AdapterResolution, AdapterResolutionKind, AdapterResolutionResult, AdapterResponse, AdapterRun,
    BundleKey, BundleProfile, BundleProfileError, BundleResult, BundleTarget, BundleTransformer,
    Case, CaseError, CaseResult, ExpectationError, ExpectedCount, FindingExpectation,
    FindingExpectationError, ProjectCase, SuiteReport, ToolExpectation, ToolResult,
    normalize_bundle_profiles,
};

#[cfg(test)]
mod test_support;
