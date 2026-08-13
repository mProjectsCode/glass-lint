use std::{
    ops::AddAssign,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Result;
use glass_lint_core::project::{AnalysisOperationCounts, AnalysisReport, ReportCompletion};
use glass_lint_project::ProjectPhaseTimings as ProjectPhaseTimingSnapshot;

use crate::profile::{
    config::{ProfileCorpusIdentity, ProfileWorkloadIdentity},
    metrics::{
        accumulate_report, combined_digest, evidence_order_digest, median_duration,
        report_operation_counts,
    },
};

#[derive(Clone, Debug)]
pub struct ProfileWorkloadSummary {
    pub path: PathBuf,
    pub bytes: u64,
    pub findings: usize,
    pub diagnostics: usize,
    pub measured_elapsed: Duration,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: AnalysisOperationCounts,
    pub evidence_order_digest: String,
    pub error: Option<String>,
}

impl ProfileWorkloadSummary {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            bytes: 0,
            findings: 0,
            diagnostics: 0,
            measured_elapsed: Duration::ZERO,
            completion: ReportCompletion::Complete,
            run_completions: Vec::new(),
            operation_counts: AnalysisOperationCounts::default(),
            evidence_order_digest: String::new(),
            error: None,
        }
    }

    pub(super) fn merge(&mut self, source: Self) {
        self.bytes = self.bytes.max(source.bytes);
        self.findings = self.findings.saturating_add(source.findings);
        self.diagnostics = self.diagnostics.saturating_add(source.diagnostics);
        self.measured_elapsed = self
            .measured_elapsed
            .saturating_add(source.measured_elapsed);
        self.completion = self.completion.join(source.completion);
        self.run_completions.extend(source.run_completions);
        self.operation_counts += source.operation_counts;
        self.evidence_order_digest = combined_digest(&[
            self.evidence_order_digest.clone(),
            source.evidence_order_digest,
        ]);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRepetitionSummary {
    pub duration: Duration,
    pub findings: usize,
    pub diagnostics: usize,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: AnalysisOperationCounts,
    pub evidence_order_digest: String,
}

impl ProfileRepetitionSummary {
    pub fn zero() -> Self {
        Self {
            duration: Duration::ZERO,
            findings: 0,
            diagnostics: 0,
            completion: ReportCompletion::Complete,
            run_completions: Vec::new(),
            operation_counts: AnalysisOperationCounts::default(),
            evidence_order_digest: String::new(),
        }
    }

    pub fn merge(&mut self, source: Self) {
        self.duration += source.duration;
        self.findings += source.findings;
        self.diagnostics += source.diagnostics;
        self.completion = self.completion.join(source.completion);
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
    pub operation_counts: AnalysisOperationCounts,
}

pub(super) struct ProfileProjectRun {
    pub result: ProfileWorkloadSummary,
    pub repetitions: Vec<ProfileRepetitionSummary>,
    pub phases: ProfilePhaseTimings,
    pub counts: AnalysisOperationCounts,
    pub successful_runs: usize,
}

pub(super) struct ProfileProjectRunAccumulator {
    result: ProfileWorkloadSummary,
    repetitions: Vec<ProfileRepetitionSummary>,
    phases: ProfilePhaseTimings,
    counts: AnalysisOperationCounts,
    result_evidence_digests: Vec<String>,
    successful_runs: usize,
}

impl ProfileProjectRunAccumulator {
    pub fn new(path: PathBuf, repetition_count: usize) -> Self {
        Self {
            result: ProfileWorkloadSummary::new(path),
            repetitions: vec![ProfileRepetitionSummary::zero(); repetition_count],
            phases: ProfilePhaseTimings::default(),
            counts: AnalysisOperationCounts::default(),
            result_evidence_digests: Vec::new(),
            successful_runs: 0,
        }
    }

    pub fn record_success(
        &mut self,
        iteration: usize,
        warm_up: usize,
        report: &AnalysisReport,
        outcome: &RunOutcome,
    ) {
        if iteration < warm_up {
            return;
        }

        self.successful_runs = self.successful_runs.saturating_add(1);
        let repetition = &mut self.repetitions[iteration - warm_up];
        let mut repetition_evidence_digests = Vec::new();
        accumulate_report(
            report,
            &mut repetition.findings,
            &mut repetition.diagnostics,
            &mut repetition.operation_counts,
            &mut repetition_evidence_digests,
        );
        repetition.evidence_order_digest = combined_digest(&[
            repetition.evidence_order_digest.clone(),
            outcome.evidence_order_digest.clone(),
        ]);
        repetition.run_completions.push(outcome.completion);
        accumulate_report(
            report,
            &mut self.result.findings,
            &mut self.result.diagnostics,
            &mut self.result.operation_counts,
            &mut self.result_evidence_digests,
        );
        self.result.run_completions.push(outcome.completion);
        self.result.bytes = self.result.bytes.max(outcome.bytes);
        self.result.completion = self.result.completion.join(outcome.completion);
        self.phases += outcome.phases;
        self.counts += outcome.counts;
    }

    pub fn record_error(&mut self, error: impl std::fmt::Display) {
        self.result.error = Some(format!("{error:#}"));
    }

    pub fn record_repetition_duration(&mut self, index: usize, duration: Duration) {
        self.repetitions[index].duration = duration;
    }

    pub fn finish(mut self, started: Instant) -> ProfileProjectRun {
        self.result.measured_elapsed = started.elapsed();
        self.result.evidence_order_digest = combined_digest(&self.result_evidence_digests);
        ProfileProjectRun {
            result: self.result,
            repetitions: self.repetitions,
            phases: self.phases,
            counts: self.counts,
            successful_runs: self.successful_runs,
        }
    }
}

/// Aggregated phase timings owned by the profiling harness.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProfilePhaseTimings {
    discovery: Duration,
    reads: Duration,
    analyze_source: Duration,
    resolution: Duration,
    linking: Duration,
    matching: Duration,
    total: Duration,
}

impl ProfilePhaseTimings {
    pub fn with_discovery(duration: Duration) -> Self {
        let mut timings = Self::default();
        timings.record_discovery(duration);
        timings
    }

    pub fn from_project(snapshot: ProjectPhaseTimingSnapshot) -> Self {
        Self {
            discovery: snapshot.discovery(),
            reads: snapshot.reads(),
            analyze_source: snapshot.parse_and_local_analysis(),
            resolution: snapshot.resolution(),
            linking: snapshot.linking(),
            matching: snapshot.matching(),
            total: snapshot.total(),
        }
    }

    #[must_use]
    pub fn discovery(&self) -> Duration {
        self.discovery
    }

    #[must_use]
    pub fn reads(&self) -> Duration {
        self.reads
    }

    #[must_use]
    pub fn resolution(&self) -> Duration {
        self.resolution
    }

    #[must_use]
    pub fn linking(&self) -> Duration {
        self.linking
    }

    #[must_use]
    pub fn matching(&self) -> Duration {
        self.matching
    }

    #[must_use]
    pub fn total(&self) -> Duration {
        self.total
    }

    #[must_use]
    pub fn parse_and_local_analysis(&self) -> Duration {
        self.analyze_source
    }

    #[must_use]
    pub fn linking_and_matching(&self) -> Duration {
        self.linking.saturating_add(self.matching)
    }

    pub fn record_discovery(&mut self, duration: Duration) {
        self.discovery = self.discovery.saturating_add(duration);
    }

    pub fn record_reads(&mut self, duration: Duration) {
        self.reads = self.reads.saturating_add(duration);
    }

    pub fn record_analyze_source(&mut self, duration: Duration) {
        self.analyze_source = self.analyze_source.saturating_add(duration);
    }

    pub fn record_resolution(&mut self, duration: Duration) {
        self.resolution = self.resolution.saturating_add(duration);
    }

    pub fn record_linking(&mut self, duration: Duration) {
        self.linking = self.linking.saturating_add(duration);
    }

    pub fn record_matching(&mut self, duration: Duration) {
        self.matching = self.matching.saturating_add(duration);
    }

    pub fn record_total(&mut self, duration: Duration) {
        self.total = self.total.saturating_add(duration);
    }
}

impl AddAssign for ProfilePhaseTimings {
    fn add_assign(&mut self, rhs: Self) {
        self.discovery = self.discovery.saturating_add(rhs.discovery);
        self.reads = self.reads.saturating_add(rhs.reads);
        self.analyze_source = self.analyze_source.saturating_add(rhs.analyze_source);
        self.resolution = self.resolution.saturating_add(rhs.resolution);
        self.linking = self.linking.saturating_add(rhs.linking);
        self.matching = self.matching.saturating_add(rhs.matching);
        self.total = self.total.saturating_add(rhs.total);
    }
}

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
    pub counts: AnalysisOperationCounts,
    pub completion: ReportCompletion,
    pub evidence_order_digest: String,
}

impl Default for RunOutcome {
    fn default() -> Self {
        Self {
            bytes: 0,
            phases: ProfilePhaseTimings::default(),
            counts: AnalysisOperationCounts::default(),
            completion: ReportCompletion::Complete,
            evidence_order_digest: String::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct MeasuredRepetitionAccumulator {
    repetitions: Vec<ProfileRepetitionSummary>,
}

impl MeasuredRepetitionAccumulator {
    pub(super) fn with_repetitions(repetition_count: usize) -> Self {
        Self {
            repetitions: vec![ProfileRepetitionSummary::zero(); repetition_count],
        }
    }

    pub(super) fn measure<W, R>(
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

    pub(super) fn record(&mut self, repetition: ProfileRepetitionSummary) {
        self.repetitions.push(repetition);
    }

    pub(super) fn merge_project(&mut self, project: Vec<ProfileRepetitionSummary>) -> Result<()> {
        if project.len() != self.repetitions.len() {
            anyhow::bail!(
                "profile repetition count changed while merging project: expected {}, got {}",
                self.repetitions.len(),
                project.len()
            );
        }
        for (target, source) in self.repetitions.iter_mut().zip(project) {
            target.merge(source);
        }
        Ok(())
    }

    pub(super) fn findings(&self) -> usize {
        self.repetitions
            .iter()
            .map(|repetition| repetition.findings)
            .sum()
    }

    pub(super) fn diagnostics(&self) -> usize {
        self.repetitions
            .iter()
            .map(|repetition| repetition.diagnostics)
            .sum()
    }

    pub(super) fn operation_counts(&self) -> AnalysisOperationCounts {
        self.repetitions.iter().fold(
            AnalysisOperationCounts::default(),
            |mut total, repetition| {
                total += repetition.operation_counts;
                total
            },
        )
    }

    pub(super) fn median_duration(&self) -> Duration {
        median_duration(&self.repetitions)
    }

    pub(super) fn total_duration(&self) -> Duration {
        self.repetitions
            .iter()
            .map(|repetition| repetition.duration)
            .sum()
    }

    pub(super) fn into_repetitions(self) -> Vec<ProfileRepetitionSummary> {
        self.repetitions
    }

    #[cfg(test)]
    pub(super) fn repetitions(&self) -> &[ProfileRepetitionSummary] {
        &self.repetitions
    }
}

pub(super) fn project_run_outcome(
    report: &AnalysisReport,
    metrics: &glass_lint_project::ProjectLoadMetrics,
) -> RunOutcome {
    RunOutcome {
        bytes: metrics.bytes(),
        phases: ProfilePhaseTimings::from_project(metrics.phase_timings()),
        counts: report_operation_counts(report),
        completion: report.completion(),
        evidence_order_digest: evidence_order_digest(report),
    }
}

pub(super) struct PreparedFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub source: String,
}

/// Workload-specific identity, timings, and result data that finalize a
/// `ProfileSummary` from accumulated totals.
pub(super) struct ProfileSummaryMetadata {
    pub workload: ProfileWorkloadIdentity,
    pub setup_duration: Duration,
    pub measured_elapsed: Duration,
    pub wall_duration: Duration,
    pub repetitions: Vec<ProfileRepetitionSummary>,
    pub phase_timings: ProfilePhaseTimings,
    pub operation_counts: AnalysisOperationCounts,
}

#[derive(Default)]
pub(super) struct ProfileSummaryAccumulator {
    workload_results: Vec<ProfileWorkloadSummary>,
    files: usize,
    bytes: u64,
    findings: usize,
    diagnostics: usize,
    errors: usize,
    runs: usize,
}

impl ProfileSummaryAccumulator {
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

    pub fn finish(self, metadata: ProfileSummaryMetadata) -> ProfileSummary {
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
