use std::{path::PathBuf, time::Duration};

use glass_lint_core::project::AnalysisOperationCounts;

use crate::profile::{
    config::ProfileWorkloadIdentity,
    metrics::median_duration,
    types::{
        ProfilePhaseTimings, ProfileRepetitionSummary, ProfileSummary, ProfileWorkloadSummary,
    },
};

pub(in crate::profile) struct PreparedFile {
    pub(in crate::profile) path: PathBuf,
    pub(in crate::profile) bytes: u64,
    pub(in crate::profile) source: String,
}

/// Workload-specific identity, timings, and result data that finalize a
/// `ProfileSummary` from accumulated totals.
pub(in crate::profile) struct ProfileSummaryMetadata {
    pub(in crate::profile) workload: ProfileWorkloadIdentity,
    pub(in crate::profile) setup_duration: Duration,
    pub(in crate::profile) measured_elapsed: Duration,
    pub(in crate::profile) wall_duration: Duration,
    pub(in crate::profile) repetitions: Vec<ProfileRepetitionSummary>,
    pub(in crate::profile) phase_timings: ProfilePhaseTimings,
    pub(in crate::profile) operation_counts: AnalysisOperationCounts,
}

#[derive(Default)]
pub(in crate::profile) struct ProfileSummaryAccumulator {
    workload_results: Vec<ProfileWorkloadSummary>,
    files: usize,
    bytes: u64,
    findings: usize,
    diagnostics: usize,
    errors: usize,
    runs: usize,
}

impl ProfileSummaryAccumulator {
    pub(in crate::profile) fn record(
        &mut self,
        result: ProfileWorkloadSummary,
        successful_runs: usize,
    ) {
        self.files = self.files.saturating_add(1);
        self.bytes = self.bytes.saturating_add(result.bytes);
        self.findings = self.findings.saturating_add(result.findings);
        self.diagnostics = self.diagnostics.saturating_add(result.diagnostics);
        self.errors = self
            .errors
            .saturating_add(usize::from(result.error.is_some()));
        self.runs = self.runs.saturating_add(successful_runs);
        self.workload_results.push(result);
    }

    pub(in crate::profile) fn finish(self, metadata: ProfileSummaryMetadata) -> ProfileSummary {
        let median_repetition_duration = median_duration(&metadata.repetitions);
        ProfileSummary {
            workload: metadata.workload,
            inputs: self.files,
            bytes: self.bytes,
            findings: self.findings,
            diagnostics: self.diagnostics,
            errors: self.errors,
            runs: self.runs,
            setup_duration: metadata.setup_duration,
            measured_elapsed: metadata.measured_elapsed,
            wall_duration: metadata.wall_duration,
            repetitions: metadata.repetitions,
            median_repetition_duration,
            workload_results: self.workload_results,
            phase_timings: metadata.phase_timings,
            operation_counts: metadata.operation_counts,
        }
    }
}
