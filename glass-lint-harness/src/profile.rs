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
use glass_lint_core::{Linter, RuleId};
use glass_lint_project::{ProjectLoadOptions, ProjectLoader, ProjectSelection, SourceCorpus};
use glob::{MatchOptions, Pattern};

use crate::builtins::{self, BuiltInProfile};

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
    pub setup_elapsed: Duration,
    /// Wall time for the measured linting phase.
    pub elapsed: Duration,
    pub total_elapsed: Duration,
    pub file_results: Vec<ProfileFileSummary>,
    pub phase_timings: ProfilePhaseTimings,
    pub operation_counts: ProfileOperationCounts,
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

#[derive(Clone, Copy, Debug, Default)]
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
        report: &glass_lint_core::ProjectReport,
        metrics: &glass_lint_project::ProjectLoadMetrics,
    ) -> Self {
        Self {
            files: metrics.files,
            requests: metrics.requests,
            edges: metrics.edges,
            exports: report.operations.exports,
            scc_rounds: report.operations.scc_rounds,
            effect_projections: report.operations.effect_projections,
            evidence: report.operations.evidence,
        }
    }
}

#[derive(Default)]
struct RunOutcome {
    findings: usize,
    diagnostics: usize,
    bytes: u64,
    phases: ProfilePhaseTimings,
    counts: ProfileOperationCounts,
}

fn project_run_outcome(
    report: &glass_lint_core::ProjectReport,
    metrics: &glass_lint_project::ProjectLoadMetrics,
) -> RunOutcome {
    RunOutcome {
        findings: report.files.iter().map(|file| file.findings.len()).sum(),
        diagnostics: report.diagnostics.len()
            + report
                .files
                .iter()
                .map(|file| file.parse_diagnostics.len())
                .sum::<usize>(),
        bytes: metrics.bytes,
        phases: ProfilePhaseTimings::from_project_metrics(metrics),
        counts: ProfileOperationCounts::from_project(report, metrics),
    }
}

struct ProfileLinter(Arc<Linter>);

struct PreparedFile {
    path: PathBuf,
    bytes: u64,
    source: String,
}

pub fn profile_folder(config: &ProfileConfig) -> Result<ProfileSummary> {
    // Validate before discovery so invalid runs cannot partially consume a
    // corpus or report misleading timing totals.
    validate_config(config)?;
    if config.project {
        return profile_projects(config);
    }
    let total_start = Instant::now();
    let mut paths = discover_profile_files(&config.paths, &config.include, &config.exclude)?;
    if let Some(sample) = config.sample {
        sample_paths(&mut paths, sample, config.seed);
    }
    let linters = Arc::new(build_linters(config.provider, config.mode, &config.rules)?);

    // Keep discovery, metadata, and UTF-8 decoding outside the measured
    // workload. This also leaves Samply with a memory-resident corpus.
    let setup_start = Instant::now();
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
    let (results, lint_elapsed) = run_profile(
        &prepared,
        &linters,
        config.workers,
        config.warm_up,
        config.repeat,
    );

    file_results.extend(results);
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
        file_results,
        phase_timings: ProfilePhaseTimings {
            discovery: setup_elapsed,
            matching: lint_elapsed,
            total: total_start.elapsed(),
            ..ProfilePhaseTimings::default()
        },
        operation_counts: ProfileOperationCounts {
            files: paths.len(),
            evidence: findings,
            ..ProfileOperationCounts::default()
        },
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
        for iteration in 0..config.warm_up + config.repeat {
            for ProfileLinter(linter) in &linters {
                match loader.load_and_lint_with_metrics(linter, &selection) {
                    Ok((report, metrics)) => {
                        if iteration >= config.warm_up {
                            let outcome = project_run_outcome(&report, &metrics);
                            project_findings += outcome.findings;
                            project_diagnostics += outcome.diagnostics;
                            project_files = project_files.max(outcome.counts.files);
                            project_bytes = project_bytes.max(outcome.bytes);
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
        file_results,
        phase_timings: phases,
        operation_counts: counts,
    })
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

pub fn discover_profile_files(
    roots: &[PathBuf],
    includes: &[String],
    excludes: &[String],
) -> Result<Vec<PathBuf>> {
    // BTreeMap deduplicates overlapping roots while retaining deterministic path
    // order.
    let includes = compile_globs(includes)?;
    let excludes = compile_globs(excludes)?;
    // Folder profiling samples after discovery, so the project admission cap
    // must not reject a corpus before `--sample` can reduce it. Traversal is
    // still bounded by `max_visited_entries`.
    let corpus_options = ProjectLoadOptions {
        max_files: usize::MAX,
        ..ProjectLoadOptions::default()
    };
    let corpus = SourceCorpus::new(&corpus_options)?;
    let mut paths = BTreeMap::<PathBuf, ()>::new();
    for root in roots {
        let found = corpus.discover_filtered(std::slice::from_ref(root), |path| {
            matches_filters(path, root, &includes, &excludes)
        })?;
        paths.extend(found.into_iter().map(|path| (path, ())));
    }
    Ok(paths.into_keys().collect())
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
    Ok(())
}

fn matches_filters(path: &Path, root: &Path, includes: &[Pattern], excludes: &[Pattern]) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative = relative.to_string_lossy().replace('\\', "/");
    let basename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let matches = |patterns: &[Pattern]| {
        patterns.iter().any(|pattern| {
            pattern.matches_with(&relative, options)
                || (!pattern.as_str().contains('/') && pattern.matches_with(&basename, options))
        })
    };
    (includes.is_empty() || matches(includes)) && !matches(excludes)
}

fn compile_globs(patterns: &[String]) -> Result<Vec<Pattern>> {
    patterns
        .iter()
        .map(|pattern| {
            Pattern::new(pattern).with_context(|| format!("compile profiling glob {pattern}"))
        })
        .collect()
}

fn sample_paths(paths: &mut Vec<PathBuf>, sample: usize, seed: u64) {
    // Use a small deterministic PRNG and sort the retained sample for stable
    // output even though selection itself is randomized by the seed.
    if sample >= paths.len() {
        return;
    }
    let mut state = if seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        seed
    };
    for index in (1..paths.len()).rev() {
        state ^= state << 7;
        state ^= state >> 9;
        state ^= state << 8;
        paths.swap(index, (state as usize) % (index + 1));
    }
    paths.truncate(sample);
    paths.sort();
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
    for iteration in 0..warm_up + repeat {
        for ProfileLinter(linter) in linters {
            let started = Instant::now();
            let report = linter.lint(&file.source, &file.path.to_string_lossy());
            if iteration >= warm_up {
                elapsed += started.elapsed();
                findings += report.findings.len();
                diagnostics += report.parse_diagnostics.len();
            }
        }
    }
    ProfileFileSummary {
        path: file.path.clone(),
        bytes: file.bytes,
        findings,
        diagnostics,
        elapsed,
        error: None,
    }
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
    (
        Arc::try_unwrap(results)
            .expect("profile workers still hold result storage")
            .into_inner()
            .expect("profile result storage was poisoned"),
        elapsed,
    )
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
