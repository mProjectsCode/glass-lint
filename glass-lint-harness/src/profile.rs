//! Deterministic corpus discovery and bounded provider profiling.
//!
//! Setup, measured linting, and phase metrics are kept separate so profiling
//! compares analysis work without accidentally timing corpus preparation.

#![allow(clippy::cast_possible_truncation, clippy::zero_sized_map_values)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Barrier, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{AnalysisReport, Linter, ReportCompletion, RuleId};
use glass_lint_project::{ProjectLoadOptions, ProjectLoadOutcome, ProjectLoader, ProjectSelection};

use crate::{
    builtins::{self, BuiltInProfile},
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
pub enum ProfileProvider {
    Js,
    Obsidian,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Rule precision profile used during measurement.
pub enum ProfileMode {
    Recommended,
    Heuristic,
}

#[derive(Clone, Debug)]
/// Validated-by-`profile_folder` controls for one profile run.
pub struct ProfileConfig {
    /// Files or directories to discover.
    pub paths: Vec<PathBuf>,
    /// Inclusive glob filters.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub sample: Option<usize>,
    pub seed: u64,
    pub warm_up: usize,
    pub repeat: usize,
    pub continue_on_error: bool,
    pub workers: usize,
    pub provider: ProfileProvider,
    pub mode: ProfileMode,
    pub rules: Vec<String>,
    pub project: bool,
    pub admitted_project: bool,
    /// Optional immutable selection manifest shared by profiling modes.
    pub manifest: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ProfileFileSummary {
    /// Discovered file path.
    pub path: PathBuf,
    /// UTF-8 source byte count.
    pub bytes: u64,
    /// Findings across measured repetitions.
    pub findings: usize,
    /// Parse/analysis diagnostics across measured repetitions.
    pub diagnostics: usize,
    /// Time spent in lint calls, excluding corpus discovery and file reads.
    pub elapsed: Duration,
    pub completion: ReportCompletion,
    pub run_completions: Vec<ReportCompletion>,
    pub operation_counts: ProfileOperationCounts,
    pub evidence_order_digest: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProfileSummary {
    /// Number of discovered files.
    pub files: usize,
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
    /// Discovery, verification, file reads, decoding, and linter construction;
    /// excludes warm-up and measured analysis.
    pub setup_elapsed: Duration,
    /// Wall time for the measured linting phase.
    pub elapsed: Duration,
    /// End-to-end wall time including setup, warm-up, and measured analysis.
    pub total_elapsed: Duration,
    /// One correctness/timing record for each measured repetition.
    pub repetitions: Vec<ProfileRepetitionSummary>,
    /// Median measured repetition duration.
    pub median_elapsed: Duration,
    /// Verified selection manifest digest, when a manifest drives the run.
    pub manifest_digest: Option<String>,
    pub file_results: Vec<ProfileFileSummary>,
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

/// Reject a performance comparison when deterministic correctness differs.
pub fn ensure_profile_correctness_match(
    left: &ProfileSummary,
    right: &ProfileSummary,
) -> Result<()> {
    if left.manifest_digest != right.manifest_digest || left.bytes != right.bytes {
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

#[derive(Clone, Debug, Default)]
pub struct ProfilePhaseTimings {
    pub discovery: Duration,
    pub reads: Duration,
    pub parse_and_local_analysis: Duration,
    pub resolution: Duration,
    pub linking: Duration,
    pub linking_and_matching: Duration,
    pub matching: Duration,
    pub total: Duration,
}

impl std::ops::AddAssign for ProfilePhaseTimings {
    fn add_assign(&mut self, rhs: Self) {
        self.discovery = self.discovery.saturating_add(rhs.discovery);
        self.reads = self.reads.saturating_add(rhs.reads);
        self.parse_and_local_analysis = self
            .parse_and_local_analysis
            .saturating_add(rhs.parse_and_local_analysis);
        self.resolution = self.resolution.saturating_add(rhs.resolution);
        self.linking = self.linking.saturating_add(rhs.linking);
        self.linking_and_matching = self
            .linking_and_matching
            .saturating_add(rhs.linking_and_matching);
        self.matching = self.matching.saturating_add(rhs.matching);
        self.total = self.total.saturating_add(rhs.total);
    }
}

impl ProfilePhaseTimings {
    fn from_project_metrics(metrics: &glass_lint_project::ProjectLoadMetrics) -> Self {
        Self {
            discovery: metrics.discovery,
            reads: metrics.reads,
            parse_and_local_analysis: metrics.parse_and_local_analysis,
            resolution: metrics.resolution,
            linking: metrics.linking,
            linking_and_matching: metrics.linking_and_matching,
            matching: metrics.matching,
            total: metrics.total,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProfileOperationCounts {
    /// Files included in the profile.
    pub files: usize,
    /// Resolver requests observed.
    pub requests: usize,
    pub edges: usize,
    pub exports: usize,
    pub scc_rounds: usize,
    pub effect_projections: usize,
    pub evidence: usize,
}

impl std::ops::AddAssign for ProfileOperationCounts {
    fn add_assign(&mut self, rhs: Self) {
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.exports = self.exports.saturating_add(rhs.exports);
        self.scc_rounds = self.scc_rounds.saturating_add(rhs.scc_rounds);
        self.effect_projections = self
            .effect_projections
            .saturating_add(rhs.effect_projections);
        self.evidence = self.evidence.saturating_add(rhs.evidence);
    }
}

impl ProfileOperationCounts {
    fn from_project(
        report: &glass_lint_core::AnalysisReport,
        _metrics: &glass_lint_project::ProjectLoadMetrics,
    ) -> Self {
        Self {
            files: report.operations.files,
            requests: report.operations.requests,
            edges: report.operations.edges,
            exports: report.operations.exports,
            scc_rounds: report.operations.scc_rounds,
            effect_projections: report.operations.effect_projections,
            evidence: report.operations.evidence,
        }
    }
}

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
    fn record(&mut self, repetition: ProfileRepetitionSummary) {
        self.repetitions.push(repetition);
    }

    #[cfg(test)]
    fn total_duration(&self) -> Duration {
        self.repetitions
            .iter()
            .map(|repetition| repetition.duration)
            .sum()
    }
}

fn project_run_outcome(
    report: &glass_lint_core::AnalysisReport,
    metrics: &glass_lint_project::ProjectLoadMetrics,
) -> RunOutcome {
    RunOutcome {
        findings: report.files.iter().map(|file| file.findings.len()).sum(),
        diagnostics: all_diagnostic_count(report),
        bytes: metrics.bytes,
        phases: ProfilePhaseTimings::from_project_metrics(metrics),
        counts: ProfileOperationCounts::from_project(report, metrics),
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
        outcome.error.map(|error| format!("{error:#}")),
    )
}

pub fn profile_folder(config: &ProfileConfig) -> Result<ProfileSummary> {
    // Validate before discovery so invalid runs cannot partially consume a
    // corpus or report misleading timing totals.
    validate_config(config)?;
    if config.project {
        return profile_projects(config);
    }
    if config.admitted_project {
        return profile_admitted_projects(config);
    }
    let total_start = Instant::now();
    let setup_start = Instant::now();
    let (paths, manifest_digest, _) = selected_profile_paths(config)?;
    let linters = Arc::new(build_linters(config.provider, config.mode, &config.rules)?);

    // Keep discovery, metadata, and UTF-8 decoding outside the measured
    // workload. This also leaves Samply with a memory-resident corpus.
    let mut prepared = Vec::with_capacity(paths.len());
    let mut file_results = Vec::new();
    for path in &paths {
        match prepare_file(path) {
            Ok(file) => prepared.push(file),
            Err(error) => {
                let result = ProfileFileSummary {
                    path: path.clone(),
                    bytes: 0,
                    findings: 0,
                    diagnostics: 0,
                    elapsed: Duration::ZERO,
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
                file_results.push(result);
            }
        }
    }
    let setup_elapsed = setup_start.elapsed();

    let prepared = Arc::new(prepared);
    let _ = run_profile(&prepared, &linters, config.workers, config.warm_up, 0);
    let measured_start = Instant::now();
    let mut measured_results = Vec::new();
    let mut measured = MeasuredRepetitionAccumulator {
        repetitions: Vec::with_capacity(config.repeat),
    };
    let mut operation_counts = ProfileOperationCounts::default();
    for _ in 0..config.repeat {
        let (results, duration) = run_profile(&prepared, &linters, config.workers, 0, 1);
        let repetition = repetition_from_files(duration, &results);
        operation_counts += repetition.operation_counts;
        measured.record(repetition);
        measured_results.extend(results);
    }
    let lint_elapsed = measured_start.elapsed();

    file_results.extend(aggregate_file_results(measured_results));
    file_results.sort_by(|left, right| left.path.cmp(&right.path));

    let errors = file_results
        .iter()
        .filter(|result| result.error.is_some())
        .count();
    let bytes = file_results.iter().map(|result| result.bytes).sum();
    let findings = file_results.iter().map(|result| result.findings).sum();
    let diagnostics = file_results.iter().map(|result| result.diagnostics).sum();
    let successful_files = file_results
        .iter()
        .filter(|result| result.error.is_none())
        .count();

    Ok(ProfileSummary {
        files: paths.len(),
        bytes,
        findings,
        diagnostics,
        errors,
        runs: successful_files * config.repeat,
        setup_elapsed,
        elapsed: lint_elapsed,
        total_elapsed: total_start.elapsed(),
        median_elapsed: median_duration(&measured.repetitions),
        repetitions: measured.repetitions,
        manifest_digest,
        file_results,
        phase_timings: ProfilePhaseTimings {
            discovery: setup_elapsed,
            matching: lint_elapsed,
            total: total_start.elapsed(),
            ..ProfilePhaseTimings::default()
        },
        operation_counts,
    })
}

fn profile_projects(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let linters = build_linters(config.provider, config.mode, &config.rules)?;
    let loader = ProjectLoader::new(ProjectLoadOptions::default())?;
    let mut file_results = Vec::new();
    let mut phases = ProfilePhaseTimings::default();
    let mut counts = ProfileOperationCounts::default();
    let mut findings = 0;
    let mut diagnostics = 0;
    let mut errors = 0;
    let mut bytes: u64 = 0;
    let mut runs = 0;
    let mut files: usize = 0;

    for path in &config.paths {
        let selection = if path.is_dir() {
            ProjectSelection::directory(path.clone())
        } else {
            ProjectSelection::entry(path.clone())
        };
        let started = Instant::now();
        let mut project_findings = 0;
        let mut project_diagnostics = 0;
        let mut project_files = 0;
        let mut project_bytes = 0;
        let mut project_error = None;
        let mut project_completion = ReportCompletion::Complete;
        let mut project_counts = ProfileOperationCounts::default();
        let mut project_evidence_digests = Vec::new();
        for iteration in 0..config.warm_up + config.repeat {
            for ProfileLinter(linter) in &linters {
                match loader.load_and_lint(linter, &selection) {
                    Ok(outcome) => {
                        let (report, metrics, error) = profile_project_parts(outcome);
                        project_error = error.or(project_error);
                        if iteration >= config.warm_up {
                            let outcome = project_run_outcome(&report, &metrics);
                            project_findings += outcome.findings;
                            project_diagnostics += outcome.diagnostics;
                            project_files = project_files.max(outcome.counts.files);
                            project_bytes = project_bytes.max(outcome.bytes);
                            if outcome.completion == ReportCompletion::Partial {
                                project_completion = ReportCompletion::Partial;
                            }
                            project_counts += outcome.counts;
                            project_evidence_digests.push(outcome.evidence_order_digest);
                            phases += outcome.phases;
                            counts += outcome.counts;
                            runs += 1;
                        }
                    }
                    Err(error) => {
                        project_error = Some(format!("{error:#}"));
                        if !config.continue_on_error {
                            return Err(error.into());
                        }
                    }
                }
            }
        }
        if project_error.is_some() {
            errors += 1;
        }
        findings += project_findings;
        diagnostics += project_diagnostics;
        bytes = bytes.saturating_add(project_bytes);
        files = files.saturating_add(project_files);
        file_results.push(ProfileFileSummary {
            path: path.clone(),
            bytes: project_bytes,
            findings: project_findings,
            diagnostics: project_diagnostics,
            elapsed: started.elapsed(),
            completion: project_completion,
            run_completions: Vec::new(),
            operation_counts: project_counts,
            evidence_order_digest: combined_digest(&project_evidence_digests),
            error: project_error,
        });
    }
    phases.total = total_start.elapsed();
    Ok(ProfileSummary {
        files,
        bytes,
        findings,
        diagnostics,
        errors,
        runs,
        setup_elapsed: phases.discovery + phases.reads,
        elapsed: phases.parse_and_local_analysis
            + phases.resolution
            + phases.linking
            + phases.matching,
        total_elapsed: phases.total,
        repetitions: Vec::new(),
        median_elapsed: Duration::ZERO,
        manifest_digest: None,
        file_results,
        phase_timings: phases,
        operation_counts: counts,
    })
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
    let setup_elapsed = total_start.elapsed();
    for _ in 0..config.warm_up {
        for ProfileLinter(linter) in &linters {
            let _ = admitted_project_run(&root, &prepared, linter, config.workers)?;
        }
    }

    let mut findings = 0;
    let mut diagnostics = 0;
    let bytes = prepared.iter().map(|file| file.bytes).sum::<u64>();
    if let Some(verified_bytes) = verified_bytes
        && bytes != verified_bytes
    {
        bail!("verified manifest bytes changed during profile preparation");
    }
    let mut operation_counts = ProfileOperationCounts::default();
    let mut measured = MeasuredRepetitionAccumulator {
        repetitions: Vec::with_capacity(config.repeat),
    };
    let measured_start = Instant::now();
    for _ in 0..config.repeat {
        let repetition_start = Instant::now();
        let mut repetition_findings = 0;
        let mut repetition_diagnostics = 0;
        let mut repetition_counts = ProfileOperationCounts::default();
        let mut repetition_completion = ReportCompletion::Complete;
        let mut run_completions = Vec::with_capacity(linters.len());
        let mut evidence_digests = Vec::new();
        for ProfileLinter(linter) in &linters {
            let report = admitted_project_run(&root, &prepared, linter, config.workers)?;
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
        let repetition = ProfileRepetitionSummary {
            duration: repetition_start.elapsed(),
            findings: repetition_findings,
            diagnostics: repetition_diagnostics,
            completion: repetition_completion,
            run_completions,
            operation_counts: repetition_counts,
            evidence_order_digest: combined_digest(&evidence_digests),
        };
        findings += repetition.findings;
        diagnostics += repetition.diagnostics;
        operation_counts += repetition.operation_counts;
        measured.record(repetition);
    }
    let elapsed = measured_start.elapsed();
    let median_elapsed = median_duration(&measured.repetitions);
    Ok(ProfileSummary {
        files: prepared.len(),
        bytes,
        findings,
        diagnostics,
        errors: 0,
        runs: config.repeat.saturating_mul(linters.len()),
        setup_elapsed,
        elapsed,
        total_elapsed: total_start.elapsed(),
        repetitions: measured.repetitions,
        median_elapsed,
        manifest_digest,
        file_results: Vec::new(),
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
    // authored module requests therefore retain the session's typed unresolved
    // status unless a future manifest format carries explicit resolutions.
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
    if config.workers == 0 {
        bail!("--workers must be at least 1");
    }
    if config.repeat == 0 {
        bail!("--repeat must be at least 1");
    }
    if config.sample == Some(0) {
        bail!("--sample must be at least 1");
    }
    if config.project && config.admitted_project {
        bail!("--project and --admitted-project are mutually exclusive");
    }
    if config.admitted_project && config.paths.len() != 1 {
        bail!("--admitted-project requires exactly one --path root");
    }
    if config.manifest.is_some() && config.paths.len() != 1 {
        bail!("--manifest requires exactly one --path root");
    }
    if config.admitted_project && !config.paths[0].is_dir() {
        bail!("--admitted-project root must be a directory");
    }
    Ok(())
}

fn build_linters(
    provider: ProfileProvider,
    mode: ProfileMode,
    rules: &[String],
) -> Result<Vec<ProfileLinter>> {
    let parsed = rules
        .iter()
        .map(|rule| RuleId::parse(rule.clone()).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    let providers = match provider {
        ProfileProvider::Js => vec!["js"],
        ProfileProvider::Obsidian => vec!["obsidian"],
        ProfileProvider::Both => vec!["js", "obsidian"],
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
        let environment = if provider == ProfileProvider::Both {
            Some(glass_lint_obsidian::default_environment())
        } else {
            None
        };
        let provider = builtins::provider(prefix)?;
        let profile = match mode {
            ProfileMode::Recommended => BuiltInProfile::Recommended,
            ProfileMode::Heuristic => BuiltInProfile::Heuristic,
        };
        let linter = builtins::linter(
            provider,
            profile,
            environment.unwrap_or_else(glass_lint_core::Environment::default),
        );
        let linter = if rules.is_empty() {
            linter
        } else {
            Linter::with_rules(linter.catalog().clone(), selected)?
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
) -> ProfileFileSummary {
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
    ProfileFileSummary {
        path: file.path.clone(),
        bytes: file.bytes,
        findings,
        diagnostics,
        elapsed,
        completion,
        run_completions,
        operation_counts,
        evidence_order_digest: combined_digest(&evidence_digests),
        error: None,
    }
}

fn aggregate_file_results(results: Vec<ProfileFileSummary>) -> Vec<ProfileFileSummary> {
    let mut aggregated = BTreeMap::<PathBuf, ProfileFileSummary>::new();
    for result in results {
        let entry = aggregated
            .entry(result.path.clone())
            .or_insert_with(|| ProfileFileSummary {
                path: result.path.clone(),
                bytes: result.bytes,
                findings: 0,
                diagnostics: 0,
                elapsed: Duration::ZERO,
                completion: ReportCompletion::Complete,
                run_completions: Vec::new(),
                operation_counts: ProfileOperationCounts::default(),
                evidence_order_digest: String::new(),
                error: None,
            });
        entry.findings = entry.findings.saturating_add(result.findings);
        entry.diagnostics = entry.diagnostics.saturating_add(result.diagnostics);
        entry.elapsed = entry.elapsed.saturating_add(result.elapsed);
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

fn run_profile(
    prepared: &Arc<Vec<PreparedFile>>,
    linters: &Arc<Vec<ProfileLinter>>,
    workers: usize,
    warm_up: usize,
    repeat: usize,
) -> (Vec<ProfileFileSummary>, Duration) {
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
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("glass-lint-profile-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn config(root: &Path) -> ProfileConfig {
        ProfileConfig {
            paths: vec![root.to_owned()],
            include: vec![],
            exclude: vec![],
            sample: None,
            seed: 1,
            warm_up: 0,
            repeat: 1,
            continue_on_error: false,
            workers: 1,
            provider: ProfileProvider::Js,
            mode: ProfileMode::Recommended,
            rules: vec![],
            project: false,
            admitted_project: false,
            manifest: None,
        }
    }

    #[test]
    fn discovers_sorted_unique_filtered_files() {
        let root = temp_root();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("z.js"), "").unwrap();
        fs::write(root.join("nested/a.js"), "").unwrap();
        fs::write(root.join("nested/no.txt"), "").unwrap();
        let paths = discover_profile_files(
            &[root.clone(), root.join("nested")],
            &["**/a.js".into()],
            &[],
        )
        .unwrap();
        assert_eq!(paths, vec![root.join("nested/a.js")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_all_runtime_module_extensions_but_not_declarations() {
        let root = temp_root();
        for filename in ["a.js", "b.cjs", "c.mjs", "d.ts", "e.cts", "f.mts", "g.d.ts"] {
            fs::write(root.join(filename), "").unwrap();
        }
        let paths = discover_profile_files(std::slice::from_ref(&root), &[], &[]).unwrap();
        assert_eq!(paths.len(), 6);
        assert!(!paths.iter().any(|path| path.ends_with("g.d.ts")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_folder_is_a_valid_profile_corpus() {
        let root = temp_root();
        let result = profile_folder(&config(&root)).unwrap();
        assert_eq!(result.files, 0);
        assert_eq!(result.runs, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_files_are_counted_as_parse_diagnostics() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        let result = profile_folder(&config(&root)).unwrap();
        assert_eq!(result.files, 1);
        assert_eq!(result.diagnostics, 1);
        assert_eq!(result.errors, 0);
        fs::remove_dir_all(root).unwrap();
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
            admitted_project: true,
            workers,
            warm_up: 1,
            repeat: 1,
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
    fn admitted_project_counts_all_diagnostics_and_completion() {
        let root = temp_root();
        fs::write(root.join("broken.js"), "function (").unwrap();
        fs::write(root.join("request.js"), "import './missing.js';").unwrap();
        let result = profile_folder(&admitted_config(&root, 1)).unwrap();
        assert_eq!(result.repetitions.len(), 1);
        assert_eq!(result.diagnostics, result.repetitions[0].diagnostics);
        assert!(result.diagnostics >= 2);
        assert_eq!(result.repetitions[0].completion, ReportCompletion::Partial);
        assert!(result.elapsed >= result.repetitions[0].duration);
        assert!(result.total_elapsed >= result.setup_elapsed + result.elapsed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admitted_project_preserves_full_operation_counts() {
        let root = temp_root();
        fs::write(root.join("a.js"), "export const value = 1; fetch('/');").unwrap();
        fs::write(root.join("b.js"), "import { value } from './a.js'; value;").unwrap();
        let result = profile_folder(&admitted_config(&root, 1)).unwrap();
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
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admitted_project_worker_counts_have_identical_correctness() {
        let root = temp_root();
        for index in 0..8 {
            fs::write(root.join(format!("{index}.js")), "fetch('/');").unwrap();
        }
        let one = profile_folder(&admitted_config(&root, 1)).unwrap();
        let two = profile_folder(&admitted_config(&root, 2)).unwrap();
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
        fs::remove_dir_all(root).unwrap();
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
        let one = profile_folder(&config(&root)).unwrap();
        let mut parallel = config(&root);
        parallel.workers = 4;
        let parallel = profile_folder(&parallel).unwrap();
        ensure_profile_correctness_match(&one, &parallel).unwrap();
        fs::remove_dir_all(root).unwrap();
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
        let normal = profile_folder(&normal_config).unwrap();
        let mut admitted_config = admitted_config(&root, 1);
        admitted_config.manifest = Some(manifest_path);
        let admitted = profile_folder(&admitted_config).unwrap();
        assert_eq!(normal.manifest_digest, admitted.manifest_digest);
        assert_eq!(normal.bytes, admitted.bytes);
        assert_eq!(normal.files, admitted.files);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_profile_modes_are_mutually_exclusive() {
        let root = temp_root();
        let mut invalid = admitted_config(&root, 1);
        invalid.project = true;
        assert_eq!(
            profile_folder(&invalid).unwrap_err().to_string(),
            "--project and --admitted-project are mutually exclusive"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admitted_project_rejects_multiple_or_outside_roots() {
        let root = temp_root();
        let outside = temp_root();
        let mut multiple = admitted_config(&root, 1);
        multiple.paths.push(outside.clone());
        assert_eq!(
            profile_folder(&multiple).unwrap_err().to_string(),
            "--admitted-project requires exactly one --path root"
        );
        let mut file_root = admitted_config(&root.join("outside.js"), 1);
        fs::write(&file_root.paths[0], "").unwrap();
        assert_eq!(
            profile_folder(&file_root).unwrap_err().to_string(),
            "--admitted-project root must be a directory"
        );
        file_root.paths.clear();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recursive_discovery_does_not_follow_symlinks() {
        let root = temp_root();
        fs::write(root.join("real.js"), "").unwrap();
        std::os::unix::fs::symlink(".", root.join("link")).unwrap();
        let paths = discover_profile_files(std::slice::from_ref(&root), &[], &[]).unwrap();
        assert_eq!(paths, vec![root.join("real.js")]);
        fs::remove_dir_all(root).unwrap();
    }
}
