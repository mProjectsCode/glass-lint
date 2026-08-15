//! Deterministic source-file discovery and bounded provider profiling.
//!
//! Setup, measured linting, and phase metrics are kept separate so profiling
//! compares analysis work without accidentally timing corpus preparation.

mod config;
mod metrics;
mod runner;
mod source_files;
mod types;

pub use config::{
    ProfileAnalysisLimits, ProfileCatalogProvider, ProfileConfig, ProfileConfigBuilder,
    ProfileCorpusIdentity, ProfileExecutionIdentity, ProfileProjectLoadIdentity, ProfileWorkload,
    ProfileWorkloadIdentity, RuleSelectionProfile,
};
pub use runner::run_profile;
pub use source_files::{discover_profile_files, sample_paths};
pub use types::{
    ProfilePhaseTimings, ProfileRepetitionSummary, ProfileSummary, ProfileWorkloadSummary,
    ensure_profile_correctness_match,
};

#[cfg(test)]
mod tests;
