//! Reusable conformance harness for cases, adapters, reports, and profiling.
//!
//! This crate keeps execution policy independent from the CLI so tests and
//! alternate front ends observe the same normalization and comparison rules.

mod adapters;
mod builtins;
mod cases;
mod profile;
mod report;
mod runner;
mod types;

pub use adapters::{Adapter, ExternalAdapter, GlassLintAdapter};
pub use cases::load_cases;
pub use profile::{
    ProfileConfig, ProfileFileSummary, ProfileMode, ProfileOperationCounts, ProfilePhaseTimings,
    ProfileProvider, ProfileSummary, discover_profile_files, profile_folder,
};
pub use report::{comparison, failure_details, markdown, report_json, summary};
pub use runner::{CaseTimings, run_suite};
pub use types::{
    ADAPTER_PROTOCOL_VERSION, AdapterFile, AdapterProject, AdapterRequest, AdapterResolution,
    AdapterResolutionKind, AdapterResolutionResult, AdapterResponse, AdapterRun, Case, CaseResult,
    DiagnosticExpectation, FindingLocation, ProjectCase, SuiteReport, ToolExpectation, ToolResult,
};
