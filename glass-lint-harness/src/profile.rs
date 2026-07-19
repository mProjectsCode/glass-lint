//! Deterministic corpus discovery and bounded provider profiling.
//!
//! Setup, measured linting, and phase metrics are kept separate so profiling
//! compares analysis work without accidentally timing corpus preparation.

#![allow(clippy::cast_possible_truncation, clippy::zero_sized_map_values)]

use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc, Barrier, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{
    AnalysisOperationCounts, AnalysisReport, Linter, LinterConfig, ReportCompletion, RuleBaseline,
    RuleId, RuleOverride, RuleSelection, RuleState,
};
use glass_lint_project::{ProjectLoadOptions, ProjectLoadOutcome, ProjectLoader, ProjectSelection};

use crate::{
    builtins::{self, BuiltinProfile},
    profile_manifest::verify_profile_manifest,
};

mod corpus;
mod metrics;

pub use corpus::{discover_profile_files, sample_paths};
use metrics::{
    all_diagnostic_count, combined_digest, evidence_order_digest, median_duration,
    repetition_from_files, report_operation_counts,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Provider set included in a profile.
pub enum ProfileCatalogProvider {
    Js,
    Obsidian,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Rule precision profile used during measurement.
pub enum RuleSelectionProfile {
    Recommended,
    Heuristic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileWorkload {
    Files,
    LoaderProject,
    AdmittedProject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileCorpusIdentity {
    /// Corpus was selected and verified by an immutable manifest.
    Verified(String),
    /// Corpus was selected without a verification manifest.
    Unverified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileWorkloadIdentity {
    pub mode: ProfileWorkload,
    pub corpus: ProfileCorpusIdentity,
}

#[derive(Clone, Debug)]
/// Validated-by-`run_profile` controls for one profile run.
pub struct ProfileConfig {
    /// Files or directories to discover.
    paths: Vec<PathBuf>,
    /// Inclusive glob filters.
    include: Vec<String>,
    exclude: Vec<String>,
    sample: Option<usize>,
    seed: u64,
    warm_up: usize,
    repeat: NonZeroUsize,
    continue_on_error: bool,
    workers: NonZeroUsize,
    provider: ProfileCatalogProvider,
    mode: RuleSelectionProfile,
    rules: Vec<String>,
    /// Unit and execution path measured by this run.
    workload: ProfileWorkload,
    /// Optional immutable selection manifest shared by profiling modes.
    manifest: Option<PathBuf>,
}

/// Validated public construction path for profile runs.
#[derive(Clone, Debug)]
pub struct ProfileConfigBuilder {
    config: ProfileConfig,
}

impl ProfileConfig {
    pub fn builder(paths: impl IntoIterator<Item = PathBuf>) -> ProfileConfigBuilder {
        ProfileConfigBuilder {
            config: Self {
                paths: paths.into_iter().collect(),
                include: Vec::new(),
                exclude: Vec::new(),
                sample: None,
                seed: 0,
                warm_up: 0,
                repeat: NonZeroUsize::MIN,
                continue_on_error: false,
                workers: NonZeroUsize::MIN,
                provider: ProfileCatalogProvider::Js,
                mode: RuleSelectionProfile::Recommended,
                rules: Vec::new(),
                workload: ProfileWorkload::Files,
                manifest: None,
            },
        }
    }
}

impl ProfileConfigBuilder {
    #[must_use]
    pub fn include(mut self, values: impl IntoIterator<Item = String>) -> Self {
        self.config.include = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn exclude(mut self, values: impl IntoIterator<Item = String>) -> Self {
        self.config.exclude = values.into_iter().collect();
        self
    }

    #[must_use]
    pub fn sample(mut self, value: Option<usize>) -> Self {
        self.config.sample = value;
        self
    }

    #[must_use]
    pub fn seed(mut self, value: u64) -> Self {
        self.config.seed = value;
        self
    }

    #[must_use]
    pub fn warm_up(mut self, value: usize) -> Self {
        self.config.warm_up = value;
        self
    }

    #[must_use]
    pub fn repeat(mut self, value: NonZeroUsize) -> Self {
        self.config.repeat = value;
        self
    }

    #[must_use]
    pub fn workers(mut self, value: NonZeroUsize) -> Self {
        self.config.workers = value;
        self
    }

    #[must_use]
    pub fn continue_on_error(mut self, value: bool) -> Self {
        self.config.continue_on_error = value;
        self
    }

    #[must_use]
    pub fn provider(mut self, value: ProfileCatalogProvider) -> Self {
        self.config.provider = value;
        self
    }

    #[must_use]
    pub fn mode(mut self, value: RuleSelectionProfile) -> Self {
        self.config.mode = value;
        self
    }

    #[must_use]
    pub fn rules(mut self, value: impl IntoIterator<Item = String>) -> Self {
        self.config.rules = value.into_iter().collect();
        self
    }

    #[must_use]
    pub fn workload(mut self, value: ProfileWorkload) -> Self {
        self.config.workload = value;
        self
    }

    #[must_use]
    pub fn manifest(mut self, value: Option<PathBuf>) -> Self {
        self.config.manifest = value;
        self
    }

    pub fn build(self) -> Result<ProfileConfig> {
        validate_config(&self.config)?;
        Ok(self.config)
    }
}

#[derive(Clone, Debug)]
pub struct ProfileWorkloadSummary {
    /// Workload input path (a source file or project root).
    pub path: PathBuf,
    /// UTF-8 source byte count.
    pub bytes: u64,
    /// Findings across measured repetitions.
    pub findings: usize,
    /// Parse/analysis diagnostics across measured repetitions.
    pub diagnostics: usize,
    /// Measured time for this workload input.
    pub measured_elapsed: Duration,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: ProfileOperationCounts,
    pub evidence_order_digest: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProfileSummary {
    /// Workload kind is part of the profiling comparison contract.
    pub workload: ProfileWorkloadIdentity,
    /// Number of workload inputs (files in file mode, roots in project modes).
    pub inputs: usize,
    /// Total bytes in prepared files.
    pub bytes: u64,
    /// Total measured findings.
    pub findings: usize,
    /// Total measured diagnostics.
    pub diagnostics: usize,
    /// Files that failed preparation or linting.
    pub errors: usize,
    /// Number of successful measured file runs.
    pub runs: usize,
    /// Discovery, verification, reads, decoding, and linter construction;
    /// excludes warm-up and measured analysis.
    pub setup_duration: Duration,
    /// Wall time for the measured workload phase.
    pub measured_elapsed: Duration,
    /// End-to-end wall time including setup, warm-up, and measured analysis.
    pub wall_duration: Duration,
    /// One correctness/timing record for each measured repetition.
    pub repetitions: Vec<ProfileRepetitionSummary>,
    /// Median measured repetition duration.
    pub median_repetition_duration: Duration,
    /// One result per workload input.
    pub workload_results: Vec<ProfileWorkloadSummary>,
    pub phase_timings: ProfilePhaseTimings,
    pub operation_counts: ProfileOperationCounts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileRepetitionSummary {
    pub duration: Duration,
    pub findings: usize,
    pub diagnostics: usize,
    pub completion: ReportCompletion,
    /// Completion recorded for every underlying analysis run in stable order.
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: ProfileOperationCounts,
    pub evidence_order_digest: String,
}

/// Reject a performance comparison when deterministic
/// correctness differs.
pub fn ensure_profile_correctness_match(
    left: &ProfileSummary,
    right: &ProfileSummary,
) -> Result<()> {
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

impl ProfileRepetitionSummary {
    fn merge(&mut self, source: Self) {
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

/// Project-owned phase metrics used directly by profiling summaries.
pub type ProfilePhaseTimings = glass_lint_project::ProjectPhaseTimings;

/// Analysis counters reused by profiling without duplicating their fields or
/// saturating aggregation semantics.
pub type ProfileOperationCounts = AnalysisOperationCounts;

struct RunOutcome {
    findings: usize,
    diagnostics: usize,
    bytes: u64,
    phases: ProfilePhaseTimings,
    counts: ProfileOperationCounts,
    completion: ReportCompletion,
    evidence_order_digest: String,
}

impl Default for RunOutcome {
    fn default() -> Self {
        Self {
            findings: 0,
            diagnostics: 0,
            bytes: 0,
            phases: ProfilePhaseTimings::default(),
            counts: ProfileOperationCounts::default(),
            completion: ReportCompletion::Complete,
            evidence_order_digest: String::new(),
        }
    }
}

#[derive(Default)]
struct MeasuredRepetitionAccumulator {
    repetitions: Vec<ProfileRepetitionSummary>,
}

impl MeasuredRepetitionAccumulator {
    fn measure<W, R>(
        warm_up: usize,
        repeat: usize,
        mut warm_up_run: W,
        mut measured_run: R,
    ) -> Result<Self>
    where
        W: FnMut() -> Result<()>,
        R: FnMut() -> Result<ProfileRepetitionSummary>,
    {
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

    fn record(&mut self, repetition: ProfileRepetitionSummary) {
        self.repetitions.push(repetition);
    }

    fn total_duration(&self) -> Duration {
        self.repetitions
            .iter()
            .map(|repetition| repetition.duration)
            .sum()
    }
}

fn sum_operation_counts(repetitions: &[ProfileRepetitionSummary]) -> ProfileOperationCounts {
    repetitions.iter().fold(
        ProfileOperationCounts::default(),
        |mut total, repetition| {
            total += repetition.operation_counts;
            total
        },
    )
}

fn project_run_outcome(
    report: &glass_lint_core::AnalysisReport,
    metrics: &glass_lint_project::ProjectLoadMetrics,
) -> RunOutcome {
    RunOutcome {
        findings: report.files.iter().map(|file| file.findings.len()).sum(),
        diagnostics: all_diagnostic_count(report),
        bytes: metrics.bytes,
        phases: metrics.phase_timings(),
        counts: report_operation_counts(report),
        completion: report.completion,
        evidence_order_digest: evidence_order_digest(report),
    }
}

struct ProfileLinter(Arc<Linter>);

struct PreparedFile {
    path: PathBuf,
    bytes: u64,
    source: String,
}

/// Shared accounting for measured file/project outputs.
///
/// Profiling modes differ only in how they produce a file summary; all of
/// them feed this accumulator so totals and error semantics cannot drift.
#[derive(Default)]
struct ProfileTotals {
    workload_results: Vec<ProfileWorkloadSummary>,
    files: usize,
    bytes: u64,
    findings: usize,
    diagnostics: usize,
    errors: usize,
    runs: usize,
}

impl ProfileTotals {
    fn record(&mut self, result: ProfileWorkloadSummary, successful_runs: usize) {
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

fn profile_project_parts(
    outcome: ProjectLoadOutcome,
) -> (
    AnalysisReport,
    glass_lint_project::ProjectLoadMetrics,
    Option<String>,
) {
    (
        outcome.report,
        outcome.metrics,
        outcome.partial_reason.map(|error| format!("{error:#}")),
    )
}

pub fn run_profile(config: &ProfileConfig) -> Result<ProfileSummary> {
    // Validate before discovery so invalid runs cannot partially consume a
    // corpus or report misleading timing totals.
    validate_config(config)?;
    match config.workload {
        ProfileWorkload::LoaderProject => return profile_projects(config),
        ProfileWorkload::AdmittedProject => return profile_admitted_projects(config),
        ProfileWorkload::Files => {}
    }
    let total_start = Instant::now();
    let setup_start = Instant::now();
    let (paths, manifest_digest, _) = selected_profile_paths(config)?;
    let linters = Arc::new(build_linters(config.provider, config.mode, &config.rules)?);

    // Keep discovery, metadata, and UTF-8 decoding outside the measured
    // workload. This also leaves Samply with a memory-resident corpus.
    let mut prepared = Vec::with_capacity(paths.len());
    let mut workload_results = Vec::new();
    for path in &paths {
        match prepare_file(path) {
            Ok(file) => prepared.push(file),
            Err(error) => {
                let result = ProfileWorkloadSummary {
                    path: path.clone(),
                    bytes: 0,
                    findings: 0,
                    diagnostics: 0,
                    measured_elapsed: Duration::ZERO,
                    completion: ReportCompletion::Partial,
                    run_completions: Vec::new(),
                    operation_counts: ProfileOperationCounts::default(),
                    evidence_order_digest: String::new(),
                    error: Some(format!("{error:#}")),
                };
                if !config.continue_on_error {
                    bail!(
                        "{}: {}",
                        result.path.display(),
                        result.error.as_deref().unwrap_or("file preparation failed")
                    );
                }
                workload_results.push(result);
            }
        }
    }
    let setup_duration = setup_start.elapsed();

    let prepared = Arc::new(prepared);
    let mut measured_results = Vec::new();
    let mut measured_results_by_run = Vec::new();
    let measured = MeasuredRepetitionAccumulator::measure(
        config.warm_up,
        config.repeat.get(),
        || {
            let _ = execute_file_profile(&prepared, &linters, config.workers.get(), 1, 0);
            Ok(())
        },
        || {
            let (results, duration) =
                execute_file_profile(&prepared, &linters, config.workers.get(), 0, 1);
            let repetition = repetition_from_files(duration, &results);
            measured_results_by_run.push(results);
            Ok(repetition)
        },
    )?;
    measured_results.extend(measured_results_by_run.into_iter().flatten());
    let lint_elapsed = measured.total_duration();

    workload_results.extend(aggregate_workload_results(measured_results));
    workload_results.sort_by(|left, right| left.path.cmp(&right.path));

    let mut totals = ProfileTotals::default();
    for result in workload_results {
        totals.record(result, config.repeat.get());
    }
    let operation_counts = sum_operation_counts(&measured.repetitions);

    Ok(ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::Files,
            corpus: manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        inputs: totals.files,
        bytes: totals.bytes,
        findings: totals.findings,
        diagnostics: totals.diagnostics,
        errors: totals.errors,
        runs: totals.runs,
        setup_duration,
        measured_elapsed: lint_elapsed,
        wall_duration: total_start.elapsed(),
        median_repetition_duration: median_duration(&measured.repetitions),
        repetitions: measured.repetitions,
        workload_results: totals.workload_results,
        phase_timings: ProfilePhaseTimings {
            discovery: setup_duration,
            matching: lint_elapsed,
            total: total_start.elapsed(),
            ..ProfilePhaseTimings::default()
        },
        operation_counts,
    })
}

fn profile_projects(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let (_, manifest_digest, _) = selected_profile_paths(config)?;
    let linters = build_linters(config.provider, config.mode, &config.rules)?;
    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated()?);
    let mut totals = ProfileTotals::default();
    let mut phases = ProfilePhaseTimings::default();
    let mut counts = ProfileOperationCounts::default();
    let mut measured = MeasuredRepetitionAccumulator {
        repetitions: vec![
            ProfileRepetitionSummary {
                duration: Duration::ZERO,
                findings: 0,
                diagnostics: 0,
                completion: ReportCompletion::Complete,
                run_completions: Vec::new(),
                operation_counts: ProfileOperationCounts::default(),
                evidence_order_digest: String::new(),
            };
            config.repeat.get()
        ],
    };

    for path in &config.paths {
        let (result, repetitions, project_phases, project_counts, successful_runs) =
            profile_loader_project(path, config, &loader, &linters)?;
        for (target, source) in measured.repetitions.iter_mut().zip(repetitions) {
            target.merge(source);
        }
        phases += project_phases;
        counts += project_counts;
        totals.record(result, successful_runs);
    }
    phases.total = total_start.elapsed();
    Ok(ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::LoaderProject,
            corpus: manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        inputs: totals.files,
        bytes: totals.bytes,
        findings: totals.findings,
        diagnostics: totals.diagnostics,
        errors: totals.errors,
        runs: totals.runs,
        setup_duration: phases.discovery + phases.reads,
        measured_elapsed: phases.parse_and_local_analysis
            + phases.resolution
            + phases.linking
            + phases.matching,
        wall_duration: phases.total,
        median_repetition_duration: median_duration(&measured.repetitions),
        repetitions: measured.repetitions,
        workload_results: totals.workload_results,
        phase_timings: phases,
        operation_counts: counts,
    })
}

fn profile_loader_project(
    path: &Path,
    config: &ProfileConfig,
    loader: &ProjectLoader,
    linters: &[ProfileLinter],
) -> Result<(
    ProfileWorkloadSummary,
    Vec<ProfileRepetitionSummary>,
    ProfilePhaseTimings,
    ProfileOperationCounts,
    usize,
)> {
    let selection = if path.is_dir() {
        ProjectSelection::directory(path.to_owned())
    } else {
        ProjectSelection::entry(path.to_owned())
    };
    let started = Instant::now();
    let mut result = ProfileWorkloadSummary {
        path: path.to_owned(),
        bytes: 0,
        findings: 0,
        diagnostics: 0,
        measured_elapsed: Duration::ZERO,
        completion: ReportCompletion::Complete,
        run_completions: Vec::new(),
        operation_counts: ProfileOperationCounts::default(),
        evidence_order_digest: String::new(),
        error: None,
    };
    let mut repetitions = vec![
        ProfileRepetitionSummary {
            duration: Duration::ZERO,
            findings: 0,
            diagnostics: 0,
            completion: ReportCompletion::Complete,
            run_completions: Vec::new(),
            operation_counts: ProfileOperationCounts::default(),
            evidence_order_digest: String::new(),
        };
        config.repeat.get()
    ];
    let mut phases = ProfilePhaseTimings::default();
    let mut counts = ProfileOperationCounts::default();
    let mut evidence_digests = Vec::new();
    let mut successful_runs = 0_usize;

    for iteration in 0..config.warm_up + config.repeat.get() {
        let repetition_start = Instant::now();
        let mut repetition = if iteration < config.warm_up {
            ProfileRepetitionSummary {
                duration: Duration::ZERO,
                findings: 0,
                diagnostics: 0,
                completion: ReportCompletion::Complete,
                run_completions: Vec::new(),
                operation_counts: ProfileOperationCounts::default(),
                evidence_order_digest: String::new(),
            }
        } else {
            repetitions[iteration - config.warm_up].clone()
        };
        for ProfileLinter(linter) in linters {
            match loader.load_and_lint(linter, &selection) {
                Ok(outcome) => {
                    let (report, metrics, error) = profile_project_parts(outcome);
                    result.error = error.or(result.error);
                    if iteration >= config.warm_up {
                        successful_runs = successful_runs.saturating_add(1);
                        let outcome = project_run_outcome(&report, &metrics);
                        repetition.findings += outcome.findings;
                        repetition.diagnostics += outcome.diagnostics;
                        repetition.run_completions.push(outcome.completion);
                        result.run_completions.push(outcome.completion);
                        repetition.operation_counts += outcome.counts;
                        repetition.evidence_order_digest = combined_digest(&[
                            repetition.evidence_order_digest,
                            outcome.evidence_order_digest.clone(),
                        ]);
                        result.findings += outcome.findings;
                        result.diagnostics += outcome.diagnostics;
                        result.bytes = result.bytes.max(outcome.bytes);
                        if outcome.completion == ReportCompletion::Partial {
                            result.completion = ReportCompletion::Partial;
                        }
                        result.operation_counts += outcome.counts;
                        evidence_digests.push(outcome.evidence_order_digest);
                        phases += outcome.phases;
                        counts += outcome.counts;
                    }
                }
                Err(error) => {
                    result.error = Some(format!("{error:#}"));
                    if !config.continue_on_error {
                        return Err(error.into());
                    }
                }
            }
        }
        if iteration >= config.warm_up {
            repetition.duration = repetition_start.elapsed();
            repetitions[iteration - config.warm_up] = repetition;
        }
    }
    result.measured_elapsed = started.elapsed();
    result.evidence_order_digest = combined_digest(&evidence_digests);
    Ok((result, repetitions, phases, counts, successful_runs))
}

fn profile_admitted_projects(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let root = fs::canonicalize(
        config
            .paths
            .first()
            .context("admitted-project requires one root")?,
    )?;
    let (paths, manifest_digest, verified_bytes) = selected_profile_paths(config)?;
    let prepared = paths
        .iter()
        .map(|path| prepare_file(path))
        .collect::<Result<Vec<_>>>()?;
    let linters = build_linters(config.provider, config.mode, &config.rules)?;
    let setup_duration = total_start.elapsed();
    let mut findings = 0;
    let mut diagnostics = 0;
    let bytes = prepared.iter().map(|file| file.bytes).sum::<u64>();
    if let Some(verified_bytes) = verified_bytes
        && bytes != verified_bytes
    {
        bail!("verified manifest bytes changed during profile preparation");
    }
    let measured = MeasuredRepetitionAccumulator::measure(
        config.warm_up,
        config.repeat.get(),
        || {
            for ProfileLinter(linter) in &linters {
                let _ = admitted_project_run(&root, &prepared, linter, config.workers.get())?;
            }
            Ok(())
        },
        || {
            let mut repetition_findings = 0;
            let mut repetition_diagnostics = 0;
            let mut repetition_counts = ProfileOperationCounts::default();
            let mut repetition_completion = ReportCompletion::Complete;
            let mut run_completions = Vec::with_capacity(linters.len());
            let mut evidence_digests = Vec::new();
            for ProfileLinter(linter) in &linters {
                let report = admitted_project_run(&root, &prepared, linter, config.workers.get())?;
                repetition_findings += report
                    .files
                    .iter()
                    .map(|file| file.findings.len())
                    .sum::<usize>();
                repetition_diagnostics += all_diagnostic_count(&report);
                repetition_counts += report_operation_counts(&report);
                if report.completion == ReportCompletion::Partial {
                    repetition_completion = ReportCompletion::Partial;
                }
                run_completions.push(report.completion);
                evidence_digests.push(evidence_order_digest(&report));
            }
            Ok(ProfileRepetitionSummary {
                duration: Duration::ZERO,
                findings: repetition_findings,
                diagnostics: repetition_diagnostics,
                completion: repetition_completion,
                run_completions,
                operation_counts: repetition_counts,
                evidence_order_digest: combined_digest(&evidence_digests),
            })
        },
    )?;
    for repetition in &measured.repetitions {
        findings += repetition.findings;
        diagnostics += repetition.diagnostics;
    }
    let operation_counts = sum_operation_counts(&measured.repetitions);
    let elapsed = measured.total_duration();
    let median_repetition_duration = median_duration(&measured.repetitions);
    Ok(ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::AdmittedProject,
            corpus: manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        inputs: prepared.len(),
        bytes,
        findings,
        diagnostics,
        errors: 0,
        runs: config.repeat.get().saturating_mul(linters.len()),
        setup_duration,
        measured_elapsed: elapsed,
        wall_duration: total_start.elapsed(),
        repetitions: measured.repetitions,
        median_repetition_duration,
        workload_results: Vec::new(),
        phase_timings: ProfilePhaseTimings {
            parse_and_local_analysis: elapsed,
            total: total_start.elapsed(),
            ..Default::default()
        },
        operation_counts,
    })
}

fn admitted_project_run(
    root: &Path,
    prepared: &[PreparedFile],
    linter: &Linter,
    workers: usize,
) -> Result<AnalysisReport> {
    // Admitted-project profiling deliberately supplies no resolver answers;
    // Authored module requests therefore retain the session's typed unresolved
    // status; this workload intentionally measures admitted sources only.
    let mut session = linter.begin_analysis(root)?;
    for file in prepared {
        let relative = file.path.strip_prefix(root).with_context(|| {
            format!(
                "profile path outside admitted root: {}",
                file.path.display()
            )
        })?;
        session.admit_source(glass_lint_core::SourceFile::new(
            relative.to_string_lossy(),
            file.source.clone(),
        )?)?;
    }
    session.analyze_admitted_sources(workers)?;
    Ok(session.finish()?)
}

fn selected_profile_paths(
    config: &ProfileConfig,
) -> Result<(Vec<PathBuf>, Option<String>, Option<u64>)> {
    if let Some(manifest) = &config.manifest {
        let root = config
            .paths
            .first()
            .context("manifest profiling requires one root")?;
        let verified = verify_profile_manifest(root, manifest)?;
        return Ok((
            verified.paths,
            Some(verified.digest),
            Some(verified.total_bytes),
        ));
    }
    let mut paths =
        corpus::discover_profile_files(&config.paths, &config.include, &config.exclude)?;
    if let Some(sample) = config.sample {
        sample_paths(&mut paths, sample, config.seed);
    }
    Ok((paths, None, None))
}

fn prepare_file(path: &Path) -> Result<PreparedFile> {
    let metadata = fs::metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(PreparedFile {
        path: path.to_owned(),
        bytes: metadata.len(),
        source,
    })
}

fn validate_config(config: &ProfileConfig) -> Result<()> {
    if config.paths.is_empty() {
        bail!("at least one --path is required");
    }
    if config.sample == Some(0) {
        bail!("--sample must be at least 1");
    }
    if matches!(config.workload, ProfileWorkload::AdmittedProject) && config.paths.len() != 1 {
        bail!("--admitted-project requires exactly one --path root");
    }
    if config.manifest.is_some() && config.paths.len() != 1 {
        bail!("--manifest requires exactly one --path root");
    }
    if matches!(config.workload, ProfileWorkload::AdmittedProject) && !config.paths[0].is_dir() {
        bail!("--admitted-project root must be a directory");
    }
    Ok(())
}

fn build_linters(
    provider: ProfileCatalogProvider,
    mode: RuleSelectionProfile,
    rules: &[String],
) -> Result<Vec<ProfileLinter>> {
    let parsed = rules
        .iter()
        .map(|rule| RuleId::parse(rule.clone()).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    let providers = match provider {
        ProfileCatalogProvider::Js => vec!["js"],
        ProfileCatalogProvider::Obsidian => vec!["obsidian"],
        ProfileCatalogProvider::Both => vec!["js", "obsidian"],
    };
    let mut linters = Vec::new();
    for prefix in providers {
        let selected: Vec<_> = parsed
            .iter()
            .filter(|rule| rule.as_str().starts_with(&format!("{prefix}:")))
            .cloned()
            .collect();
        if !rules.is_empty() && selected.is_empty() {
            continue;
        }
        let provider = builtins::provider(prefix)?;
        let profile = match mode {
            RuleSelectionProfile::Recommended => BuiltinProfile::Recommended,
            RuleSelectionProfile::Heuristic => BuiltinProfile::Heuristic,
        };
        let linter = builtins::linter(provider, profile);
        let linter = if rules.is_empty() {
            linter
        } else {
            let selection = selected.into_iter().try_fold(
                RuleSelection::new(RuleBaseline::None),
                |selection, id| {
                    Ok::<_, glass_lint_core::LintConfigError>(
                        selection
                            .with_override(RuleOverride::new(id.to_string(), RuleState::Enabled)?),
                    )
                },
            )?;
            Linter::new(
                LinterConfig::new(
                    vec![linter.catalog().clone()],
                    linter.analysis_environment().clone(),
                )
                .with_rules(selection),
            )?
        };
        linters.push(ProfileLinter(Arc::new(linter)));
    }
    if linters.is_empty() {
        bail!("no selected rules belong to the chosen provider");
    }
    if parsed.iter().any(|rule| {
        !["js:", "obsidian:"]
            .iter()
            .any(|prefix| rule.as_str().starts_with(prefix))
    }) {
        bail!("rule does not belong to a supported profiling provider");
    }
    Ok(linters)
}

fn profile_file(
    file: &PreparedFile,
    linters: &[ProfileLinter],
    warm_up: usize,
    repeat: usize,
) -> ProfileWorkloadSummary {
    let mut findings = 0;
    let mut diagnostics = 0;
    let mut elapsed = Duration::ZERO;
    let mut completion = ReportCompletion::Complete;
    let mut run_completions = Vec::new();
    let mut operation_counts = ProfileOperationCounts::default();
    let mut evidence_digests = Vec::new();
    for iteration in 0..warm_up + repeat {
        for ProfileLinter(linter) in linters {
            let started = Instant::now();
            let filename = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("snippet.js");
            let report = linter
                .lint_snippet(&file.source, filename)
                .expect("profile paths are valid snippet project identities");
            if iteration >= warm_up {
                elapsed += started.elapsed();
                findings += report
                    .files
                    .iter()
                    .map(|file| file.findings.len())
                    .sum::<usize>();
                diagnostics += all_diagnostic_count(&report);
                if report.completion == ReportCompletion::Partial {
                    completion = ReportCompletion::Partial;
                }
                run_completions.push(report.completion);
                operation_counts += report_operation_counts(&report);
                evidence_digests.push(evidence_order_digest(&report));
            }
        }
    }
    ProfileWorkloadSummary {
        path: file.path.clone(),
        bytes: file.bytes,
        findings,
        diagnostics,
        measured_elapsed: elapsed,
        completion,
        run_completions,
        operation_counts,
        evidence_order_digest: combined_digest(&evidence_digests),
        error: None,
    }
}

fn aggregate_workload_results(results: Vec<ProfileWorkloadSummary>) -> Vec<ProfileWorkloadSummary> {
    let mut aggregated = BTreeMap::<PathBuf, ProfileWorkloadSummary>::new();
    for result in results {
        let entry =
            aggregated
                .entry(result.path.clone())
                .or_insert_with(|| ProfileWorkloadSummary {
                    path: result.path.clone(),
                    bytes: result.bytes,
                    findings: 0,
                    diagnostics: 0,
                    measured_elapsed: Duration::ZERO,
                    completion: ReportCompletion::Complete,
                    run_completions: Vec::new(),
                    operation_counts: ProfileOperationCounts::default(),
                    evidence_order_digest: String::new(),
                    error: None,
                });
        entry.findings = entry.findings.saturating_add(result.findings);
        entry.diagnostics = entry.diagnostics.saturating_add(result.diagnostics);
        entry.measured_elapsed = entry
            .measured_elapsed
            .saturating_add(result.measured_elapsed);
        if result.completion == ReportCompletion::Partial {
            entry.completion = ReportCompletion::Partial;
        }
        entry.run_completions.extend(result.run_completions);
        entry.operation_counts += result.operation_counts;
        entry.evidence_order_digest = combined_digest(&[
            entry.evidence_order_digest.clone(),
            result.evidence_order_digest,
        ]);
    }
    aggregated.into_values().collect()
}

fn execute_file_profile(
    prepared: &Arc<Vec<PreparedFile>>,
    linters: &Arc<Vec<ProfileLinter>>,
    workers: usize,
    warm_up: usize,
    repeat: usize,
) -> (Vec<ProfileWorkloadSummary>, Duration) {
    let warm_up_next = Arc::new(AtomicUsize::new(0));
    let measured_next = Arc::new(AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(Vec::with_capacity(prepared.len())));
    let warm_up_barrier = Arc::new(Barrier::new(workers));
    let measured_start = Arc::new(OnceLock::new());
    thread::scope(|scope| {
        for _ in 0..workers {
            let warm_up_next = Arc::clone(&warm_up_next);
            let measured_next = Arc::clone(&measured_next);
            let results = Arc::clone(&results);
            let prepared = Arc::clone(prepared);
            let linters = Arc::clone(linters);
            let warm_up_barrier = Arc::clone(&warm_up_barrier);
            let measured_start = Arc::clone(&measured_start);
            scope.spawn(move || {
                loop {
                    let index = warm_up_next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = prepared.get(index) else {
                        break;
                    };
                    let _ = profile_file(file, &linters, warm_up, 0);
                }
                warm_up_barrier.wait();
                measured_start.get_or_init(Instant::now);
                loop {
                    let index = measured_next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = prepared.get(index) else {
                        break;
                    };
                    let result = profile_file(file, &linters, 0, repeat);
                    results.lock().unwrap().push(result);
                }
            });
        }
    });
    let elapsed = measured_start
        .get()
        .map_or(Duration::ZERO, Instant::elapsed);
    let mut results = Arc::try_unwrap(results)
        .expect("profile workers still hold result storage")
        .into_inner()
        .expect("profile result storage was poisoned");
    results.sort_by(|left, right| left.path.cmp(&right.path));
    (results, elapsed)
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs};

    use super::*;

    fn temp_root() -> crate::test_support::TempDir {
        crate::test_support::TempDir::new()
    }

    fn config(root: &Path) -> ProfileConfig {
        ProfileConfig {
            paths: vec![root.to_owned()],
            include: vec![],
            exclude: vec![],
            sample: None,
            seed: 1,
            warm_up: 0,
            repeat: NonZeroUsize::new(1).unwrap(),
            continue_on_error: false,
            workers: NonZeroUsize::new(1).unwrap(),
            provider: ProfileCatalogProvider::Js,
            mode: RuleSelectionProfile::Recommended,
            rules: vec![],
            workload: ProfileWorkload::Files,
            manifest: None,
        }
    }

    #[test]
    fn public_profile_builder_rejects_empty_workloads() {
        let error = ProfileConfig::builder(Vec::<PathBuf>::new())
            .build()
            .unwrap_err();
        assert!(error.to_string().contains("at least one --path"));
    }

    #[test]
    fn discovers_sorted_unique_filtered_files() {
        let root = temp_root();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("z.js"), "").unwrap();
        fs::write(root.join("nested/a.js"), "").unwrap();
        fs::write(root.join("nested/no.txt"), "").unwrap();
        let paths = discover_profile_files(
            &[root.to_owned(), root.join("nested")],
            &["**/a.js".into()],
            &[],
        )
        .unwrap();
        assert_eq!(paths, vec![root.join("nested/a.js")]);
    }

    #[test]
    fn discovers_all_runtime_module_extensions_but_not_declarations() {
        let root = temp_root();
        for filename in ["a.js", "b.cjs", "c.mjs", "d.ts", "e.cts", "f.mts", "g.d.ts"] {
            fs::write(root.join(filename), "").unwrap();
        }
        let paths =
            discover_profile_files(std::slice::from_ref(&root.to_path_buf()), &[], &[]).unwrap();
        assert_eq!(paths.len(), 6);
        assert!(!paths.iter().any(|path| path.ends_with("g.d.ts")));
    }

    #[test]
    fn empty_folder_is_a_valid_profile_corpus() {
        let root = temp_root();
        let result = run_profile(&config(&root)).unwrap();
        assert_eq!(result.inputs, 0);
        assert_eq!(result.runs, 0);
    }

    #[test]
    fn malformed_files_are_counted_as_parse_diagnostics() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        let result = run_profile(&config(&root)).unwrap();
        assert_eq!(result.inputs, 1);
        assert_eq!(result.diagnostics, 1);
        assert_eq!(result.errors, 0);
    }

    #[test]
    fn sampling_is_deterministic_for_a_seed() {
        let mut left: Vec<_> = (0..20).map(|i| PathBuf::from(format!("{i}.js"))).collect();
        let mut right = left.clone();
        sample_paths(&mut left, 5, 42);
        sample_paths(&mut right, 5, 42);
        assert_eq!(left, right);
    }

    #[test]
    fn typed_accumulators_saturate_without_cross_item_bytes() {
        let mut phases = ProfilePhaseTimings {
            discovery: Duration::MAX,
            ..ProfilePhaseTimings::default()
        };
        phases += ProfilePhaseTimings {
            discovery: Duration::from_secs(1),
            ..ProfilePhaseTimings::default()
        };
        assert_eq!(phases.discovery, Duration::MAX);

        let mut counts = ProfileOperationCounts {
            files: usize::MAX,
            ..ProfileOperationCounts::default()
        };
        counts += ProfileOperationCounts {
            files: 1,
            ..ProfileOperationCounts::default()
        };
        assert_eq!(counts.files, usize::MAX);

        let first_bytes = 7_u64;
        let second_bytes = 11_u64;
        let suite_bytes = first_bytes.saturating_add(second_bytes);
        assert_eq!(first_bytes, 7);
        assert_eq!(second_bytes, 11);
        assert_eq!(suite_bytes, 18);
    }

    fn admitted_config(root: &Path, workers: usize) -> ProfileConfig {
        ProfileConfig {
            workload: ProfileWorkload::AdmittedProject,
            workers: NonZeroUsize::new(workers).unwrap(),
            warm_up: 1,
            repeat: NonZeroUsize::new(1).unwrap(),
            ..config(root)
        }
    }

    #[test]
    fn admitted_project_excludes_warmup_from_measured_duration() {
        let warmup_durations = [Duration::from_secs(11), Duration::from_secs(13)];
        let mut measured = MeasuredRepetitionAccumulator::default();
        let _ = warmup_durations;
        for duration in [Duration::from_millis(3), Duration::from_millis(7)] {
            measured.record(ProfileRepetitionSummary {
                duration,
                findings: 0,
                diagnostics: 0,
                completion: ReportCompletion::Complete,
                run_completions: vec![ReportCompletion::Complete],
                operation_counts: ProfileOperationCounts::default(),
                evidence_order_digest: String::new(),
            });
        }
        assert_eq!(measured.total_duration(), Duration::from_millis(10));
        assert_eq!(
            median_duration(&measured.repetitions),
            Duration::from_millis(3)
        );
    }

    #[test]
    fn repetition_accumulator_executes_every_requested_warmup() {
        let warmups = Cell::new(0);
        let measured = Cell::new(0);
        let result = MeasuredRepetitionAccumulator::measure(
            3,
            2,
            || {
                warmups.set(warmups.get() + 1);
                Ok(())
            },
            || {
                measured.set(measured.get() + 1);
                Ok(ProfileRepetitionSummary {
                    duration: Duration::ZERO,
                    findings: 0,
                    diagnostics: 0,
                    completion: ReportCompletion::Complete,
                    run_completions: Vec::new(),
                    operation_counts: ProfileOperationCounts::default(),
                    evidence_order_digest: String::new(),
                })
            },
        )
        .unwrap();

        assert_eq!(warmups.get(), 3);
        assert_eq!(measured.get(), 2);
        assert_eq!(result.repetitions.len(), 2);
    }

    #[test]
    fn admitted_project_counts_all_diagnostics_and_completion() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        fs::write(root.join("request.js"), "import './missing.js';").unwrap();
        let result = run_profile(&admitted_config(&root, 1)).unwrap();
        assert_eq!(result.repetitions.len(), 1);
        assert_eq!(result.diagnostics, result.repetitions[0].diagnostics);
        assert!(result.diagnostics >= 2);
        assert_eq!(result.repetitions[0].completion, ReportCompletion::Partial);
        assert!(result.measured_elapsed >= result.repetitions[0].duration);
        assert!(result.wall_duration >= result.setup_duration + result.measured_elapsed);
    }

    #[test]
    fn admitted_project_preserves_full_operation_counts() {
        let root = temp_root();
        fs::write(root.join("a.js"), "export const value = 1; fetch('/');").unwrap();
        fs::write(root.join("b.js"), "import { value } from './a.js'; value;").unwrap();
        let result = run_profile(&admitted_config(&root, 1)).unwrap();
        assert_eq!(
            result.operation_counts,
            result.repetitions[0].operation_counts
        );
        assert_eq!(result.operation_counts.files, 2);
        assert!(result.operation_counts.requests > 0);
        assert!(result.operation_counts.exports > 0);
        assert_eq!(
            result.operation_counts.effect_projections,
            result.repetitions[0].operation_counts.effect_projections
        );
        assert_eq!(
            result.operation_counts.scc_rounds,
            result.repetitions[0].operation_counts.scc_rounds
        );
    }

    #[test]
    fn admitted_project_worker_counts_have_identical_correctness() {
        let root = temp_root();
        for index in 0..8 {
            fs::write(root.join(format!("{index}.js")), "fetch('/');").unwrap();
        }
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();
        let mut first = admitted_config(&root, 1);
        first.manifest = Some(manifest.clone());
        let mut second = admitted_config(&root, 2);
        second.manifest = Some(manifest);
        let one = run_profile(&first).unwrap();
        let two = run_profile(&second).unwrap();
        assert_eq!(one.findings, two.findings);
        assert_eq!(one.diagnostics, two.diagnostics);
        assert_eq!(one.operation_counts, two.operation_counts);
        assert_eq!(one.repetitions[0].completion, two.repetitions[0].completion);
        assert_eq!(
            one.repetitions[0].evidence_order_digest,
            two.repetitions[0].evidence_order_digest
        );
        ensure_profile_correctness_match(&one, &two).unwrap();
        let mut mismatched = two;
        mismatched.repetitions[0].completion = ReportCompletion::Partial;
        assert_eq!(
            ensure_profile_correctness_match(&one, &mismatched)
                .unwrap_err()
                .to_string(),
            "profile correctness differs at repetition 1"
        );
    }

    #[test]
    fn independent_file_worker_counts_have_identical_correctness() {
        let root = temp_root();
        for (index, repetitions) in [20_000, 1, 10_000, 2, 5_000, 3].into_iter().enumerate() {
            fs::write(
                root.join(format!("{index}.js")),
                "fetch('/');\n".repeat(repetitions),
            )
            .unwrap();
        }
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();
        let mut first = config(&root);
        first.manifest = Some(manifest.clone());
        let one = run_profile(&first).unwrap();
        let mut parallel = config(&root);
        parallel.workers = NonZeroUsize::new(4).unwrap();
        parallel.manifest = Some(manifest);
        let parallel = run_profile(&parallel).unwrap();
        ensure_profile_correctness_match(&one, &parallel).unwrap();
    }

    #[test]
    fn loader_project_worker_counts_use_verified_repetitions() {
        let root = temp_root();
        fs::write(root.join("a.js"), "fetch('/');").unwrap();
        let manifest = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest).unwrap();

        let mut one = config(&root);
        one.workload = ProfileWorkload::LoaderProject;
        one.manifest = Some(manifest);
        let mut parallel = one.clone();
        parallel.workers = NonZeroUsize::new(2).unwrap();

        let one = run_profile(&one).unwrap();
        let parallel = run_profile(&parallel).unwrap();
        assert!(matches!(
            one.workload.corpus,
            ProfileCorpusIdentity::Verified(_)
        ));
        assert_eq!(one.repetitions.len(), 1);
        assert!(!one.repetitions[0].run_completions.is_empty());
        ensure_profile_correctness_match(&one, &parallel).unwrap();
    }

    #[test]
    fn normal_and_admitted_modes_use_the_same_verified_manifest() {
        let root = temp_root();
        fs::write(root.join("a.js"), "fetch('/');").unwrap();
        let manifest_path = root.join("profile-manifest.json");
        crate::create_profile_manifest(&root, &[], &[], None, 1, "fixture", &manifest_path)
            .unwrap();
        let mut normal_config = config(&root);
        normal_config.manifest = Some(manifest_path.clone());
        let normal = run_profile(&normal_config).unwrap();
        let mut admitted_config = admitted_config(&root, 1);
        admitted_config.manifest = Some(manifest_path);
        let admitted = run_profile(&admitted_config).unwrap();
        assert_eq!(normal.workload.corpus, admitted.workload.corpus);
        assert_eq!(normal.bytes, admitted.bytes);
        assert_eq!(normal.inputs, admitted.inputs);
    }

    #[test]
    fn workload_mode_is_explicit() {
        let root = temp_root();
        let config = admitted_config(&root, 1);
        assert_eq!(config.workload, ProfileWorkload::AdmittedProject);
    }

    #[test]
    fn admitted_project_rejects_multiple_or_outside_roots() {
        let root = temp_root();
        let outside = temp_root();
        let mut multiple = admitted_config(&root, 1);
        multiple.paths.push(outside.to_path_buf());
        assert_eq!(
            run_profile(&multiple).unwrap_err().to_string(),
            "--admitted-project requires exactly one --path root"
        );
        let mut file_root = admitted_config(&root.join("outside.js"), 1);
        fs::write(&file_root.paths[0], "").unwrap();
        assert_eq!(
            run_profile(&file_root).unwrap_err().to_string(),
            "--admitted-project root must be a directory"
        );
        file_root.paths.clear();
    }

    #[cfg(unix)]
    #[test]
    fn recursive_discovery_does_not_follow_symlinks() {
        let root = temp_root();
        fs::write(root.join("real.js"), "").unwrap();
        std::os::unix::fs::symlink(".", root.join("link")).unwrap();
        let paths =
            discover_profile_files(std::slice::from_ref(&root.to_path_buf()), &[], &[]).unwrap();
        assert_eq!(paths, vec![root.join("real.js")]);
    }
}
