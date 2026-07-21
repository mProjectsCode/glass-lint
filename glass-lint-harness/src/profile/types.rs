use std::{
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use glass_lint_core::{
    AnalysisOperationCounts, AnalysisReport, Linter, ReportCompletion,
};
use glass_lint_project::ProjectPhaseTimings;

use crate::profile::config::{
    ProfileCorpusIdentity, ProfileWorkloadIdentity,
};
use crate::profile::metrics::{combined_digest, evidence_order_digest, report_operation_counts};

#[derive(Clone, Debug)]
pub struct ProfileWorkloadSummary {
    pub path: PathBuf,
    pub bytes: u64,
    pub findings: usize,
    pub diagnostics: usize,
    pub measured_elapsed: Duration,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: ProfileOperationCounts,
    pub evidence_order_digest: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRepetitionSummary {
    pub duration: Duration,
    pub findings: usize,
    pub diagnostics: usize,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: ProfileOperationCounts,
    pub evidence_order_digest: String,
}

impl ProfileRepetitionSummary {
    pub fn merge(&mut self, source: Self) {
        self.duration += source.duration;
        self.findings += source.findings;
        self.diagnostics += source.diagnostics;
        if source.completion == ReportCompletion::Partial {
            self.completion = ReportCompletion::Partial;
        }
        self.run_completions.extend(source.run_completions);
        self.operation_counts += source.operation_counts;
        self.evidence_order_digest = combined_digest(&[
            self.evidence_order_digest.clone(),
            source.evidence_order_digest,
        ]);
    }
}

#[derive(Clone, Debug)]
pub struct ProfileSummary {
    pub workload: ProfileWorkloadIdentity,
    pub inputs: usize,
    pub bytes: u64,
    pub findings: usize,
    pub diagnostics: usize,
    pub errors: usize,
    pub runs: usize,
    pub setup_duration: Duration,
    pub measured_elapsed: Duration,
    pub wall_duration: Duration,
    pub repetitions: Vec<ProfileRepetitionSummary>,
    pub median_repetition_duration: Duration,
    pub workload_results: Vec<ProfileWorkloadSummary>,
    pub phase_timings: ProfilePhaseTimings,
    pub operation_counts: ProfileOperationCounts,
}

pub type ProfilePhaseTimings = ProjectPhaseTimings;
pub type ProfileOperationCounts = AnalysisOperationCounts;

pub fn ensure_profile_correctness_match(
    left: &ProfileSummary,
    right: &ProfileSummary,
) -> Result<()> {
    use anyhow::bail;

    if left.workload.mode != right.workload.mode {
        bail!("profile workload modes differ");
    }
    if !matches!(
        (&left.workload.corpus, &right.workload.corpus),
        (ProfileCorpusIdentity::Verified(left), ProfileCorpusIdentity::Verified(right))
            if left == right
    ) || left.bytes != right.bytes
    {
        bail!("profile corpus identity differs");
    }
    if left.repetitions.len() != right.repetitions.len() {
        bail!("profile repetition count differs");
    }
    for (index, (left, right)) in left.repetitions.iter().zip(&right.repetitions).enumerate() {
        if left.findings != right.findings
            || left.diagnostics != right.diagnostics
            || left.completion != right.completion
            || left.run_completions != right.run_completions
            || left.operation_counts != right.operation_counts
            || left.evidence_order_digest != right.evidence_order_digest
        {
            bail!("profile correctness differs at repetition {}", index + 1);
        }
    }
    Ok(())
}

pub(super) struct RunOutcome {
    pub bytes: u64,
    pub phases: ProfilePhaseTimings,
    pub counts: ProfileOperationCounts,
    pub completion: ReportCompletion,
    pub evidence_order_digest: String,
}

impl Default for RunOutcome {
    fn default() -> Self {
        Self {
            bytes: 0,
            phases: ProfilePhaseTimings::default(),
            counts: ProfileOperationCounts::default(),
            completion: ReportCompletion::Complete,
            evidence_order_digest: String::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct MeasuredRepetitionAccumulator {
    pub repetitions: Vec<ProfileRepetitionSummary>,
}

impl MeasuredRepetitionAccumulator {
    pub fn measure<W, R>(
        warm_up: usize,
        repeat: usize,
        mut warm_up_run: W,
        mut measured_run: R,
    ) -> Result<Self>
    where
        W: FnMut() -> Result<()>,
        R: FnMut() -> Result<ProfileRepetitionSummary>,
    {
        use std::time::Instant;

        for _ in 0..warm_up {
            warm_up_run()?;
        }
        let mut measured = Self {
            repetitions: Vec::with_capacity(repeat),
        };
        for _ in 0..repeat {
            let started = Instant::now();
            let mut repetition = measured_run()?;
            repetition.duration = started.elapsed();
            measured.record(repetition);
        }
        Ok(measured)
    }

    pub fn record(&mut self, repetition: ProfileRepetitionSummary) {
        self.repetitions.push(repetition);
    }

    pub fn total_duration(&self) -> Duration {
        self.repetitions
            .iter()
            .map(|repetition| repetition.duration)
            .sum()
    }
}

pub(super) fn sum_operation_counts(
    repetitions: &[ProfileRepetitionSummary],
) -> ProfileOperationCounts {
    repetitions.iter().fold(
        ProfileOperationCounts::default(),
        |mut total, repetition| {
            total += repetition.operation_counts;
            total
        },
    )
}

pub(super) fn project_run_outcome(
    report: &AnalysisReport,
    metrics: &glass_lint_project::ProjectLoadMetrics,
) -> RunOutcome {
    RunOutcome {
        bytes: metrics.bytes,
        phases: metrics.phase_timings(),
        counts: report_operation_counts(report),
        completion: report.completion,
        evidence_order_digest: evidence_order_digest(report),
    }
}

pub(super) struct ProfileLinter(pub Arc<Linter>);

pub(super) struct PreparedFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub source: String,
}

#[derive(Default)]
pub(super) struct ProfileTotals {
    pub workload_results: Vec<ProfileWorkloadSummary>,
    pub files: usize,
    pub bytes: u64,
    pub findings: usize,
    pub diagnostics: usize,
    pub errors: usize,
    pub runs: usize,
}

impl ProfileTotals {
    pub fn record(&mut self, result: ProfileWorkloadSummary, successful_runs: usize) {
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
}
