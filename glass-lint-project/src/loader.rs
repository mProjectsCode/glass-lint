//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, VecDeque},
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use glass_lint_core::{
    Linter,
    project::{
        AnalysisReport, AuthoredRequests, ProjectRelativePath, ResolverOutcome, SourceFile,
        SourceText,
    },
};

pub use crate::loader_metrics::{ProjectLoadMetrics, ProjectPhaseTimings};
use crate::{
    boundary::{AcceptedPaths, AcceptedSourcePath, PathClassification, SourceBoundary},
    budget::ProjectResourceBudget,
    error::ProjectLoadError,
    loader_phases::{PathWorkQueue, ResolutionCache},
    options::{ProjectSelection, ValidatedProjectLoadOptions},
    resolver::ProjectResolver,
};

mod selection;
use selection::{LoadDeadline, ProjectPaths};

/// Filesystem loader and Oxc resolver configuration.
#[derive(Clone, Debug)]
pub struct ProjectLoader {
    options: ValidatedProjectLoadOptions,
}

/// Result of a project load that may contain deterministic partial output.
#[derive(Debug)]
pub struct ProjectLoadOutcome {
    /// Completed or partial report. Timeout outcomes are returned as `Err` and
    /// never contain one.
    pub report: AnalysisReport,
    /// Source text retained from the accepted files for presentation layers.
    /// The report itself remains source-free and serializable.
    pub sources: BTreeMap<ProjectRelativePath, SourceText>,
    /// Typed completion status for the filesystem loading phase. Fatal errors,
    /// including timeout, are returned through the outer `Result`.
    status: ProjectLoadStatus,
    /// Phase timings and deterministic counters for this load.
    pub metrics: ProjectLoadMetrics,
}

/// Completion status for a project load that reached report assembly.
#[derive(Debug)]
pub enum ProjectLoadStatus {
    Complete,
    Partial { reason: ProjectLoadError },
}

impl ProjectLoadStatus {
    #[must_use]
    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }

    #[must_use]
    pub fn reason(&self) -> Option<&ProjectLoadError> {
        match self {
            Self::Complete => None,
            Self::Partial { reason } => Some(reason),
        }
    }
}

impl ProjectLoadOutcome {
    #[must_use]
    pub fn status(&self) -> &ProjectLoadStatus {
        &self.status
    }
}

impl ProjectLoadOutcome {
    fn complete(
        report: AnalysisReport,
        sources: BTreeMap<ProjectRelativePath, SourceText>,
    ) -> Self {
        Self {
            report,
            sources,
            status: ProjectLoadStatus::Complete,
            metrics: ProjectLoadMetrics::default(),
        }
    }

    fn partial(
        report: AnalysisReport,
        sources: BTreeMap<ProjectRelativePath, SourceText>,
        reason: ProjectLoadError,
    ) -> Self {
        Self {
            report: report.into_partial(&reason),
            sources,
            status: ProjectLoadStatus::Partial { reason },
            metrics: ProjectLoadMetrics::default(),
        }
    }
}

impl ProjectLoader {
    /// Construct a reusable filesystem loader from validated options.
    pub fn new(options: ValidatedProjectLoadOptions) -> Self {
        Self { options }
    }

    /// Loads, resolves, and lints one bounded project.
    pub fn load_and_lint(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
    ) -> Result<ProjectLoadOutcome, ProjectLoadError> {
        let mut metrics = ProjectLoadMetrics::default();
        let total_start = Instant::now();
        let mut outcome = self.load_project_with_outcome(linter, selection, &mut metrics)?;
        metrics.record_total(total_start.elapsed());
        outcome.metrics = metrics;
        Ok(outcome)
    }

    fn load_project_with_outcome(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<ProjectLoadOutcome, ProjectLoadError> {
        let discovery_start = Instant::now();
        let deadline = LoadDeadline::after_millis(self.options.max_timeout_ms());
        let mut budget = ProjectResourceBudget::new(
            self.options.max_visited_entries(),
            self.options.max_project_source_bytes(),
        );
        let paths = ProjectPaths::from_selection(
            &self.options,
            selection,
            deadline.instant(),
            &mut budget,
        )?;
        metrics.record_discovery(discovery_start.elapsed());

        let mut build = ProjectLoadState::new(
            linter,
            paths.boundary,
            paths.diagnostics,
            selection,
            deadline,
        )?;
        build.add_initial_paths(paths.initial_paths);
        let partial_reason = match build.close_frontier(metrics) {
            Ok(()) => {
                build.deadline.check()?;
                None
            }
            Err(ProjectLoadError::Timeout) => return Err(ProjectLoadError::Timeout),
            Err(error) => Some(error),
        };
        let (report, sources) = build.finish(metrics)?;
        Ok(match partial_reason {
            Some(reason) => ProjectLoadOutcome::partial(report, sources, reason),
            None => ProjectLoadOutcome::complete(report, sources),
        })
    }
}

/// Result of admitting and reading one work queue wave. A source-byte budget
/// failure is deferred until the successfully read sources have been
/// analyzed, preserving deterministic partial output.
struct ReadWaveOutcome {
    sources: Vec<SourceFile>,
    deferred_error: Option<ProjectLoadError>,
}

/// Maximum number of files processed in one parallel wave. Independent of
/// the total file limit so that parallelism does not create an unbounded
/// memory spike.
const WAVE_SIZE: usize = 50;

/// Mutable state for one project construction. Keeping the queue, cache, and
/// counters together makes the main loading phases explicit and auditable.
struct ProjectLoadState<'a> {
    session: glass_lint_core::project::ProjectSession<'a>,
    resolver: ProjectResolver<'a>,
    boundary: SourceBoundary<'a>,
    diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
    queue: PathWorkQueue,
    accepted: AcceptedPaths,
    resolved: ResolutionCache,
    /// Cache mapping already-accepted internal target paths to their
    /// `AcceptedSourcePath`, avoiding redundant exists/classify calls when
    /// multiple importers reference the same target.
    accepted_target_cache: BTreeMap<ProjectRelativePath, AcceptedSourcePath>,
    /// Source text retained for presentation after the core report is built.
    sources: BTreeMap<ProjectRelativePath, SourceText>,
    deadline: LoadDeadline,
}

impl<'a> ProjectLoadState<'a> {
    fn new(
        linter: &'a Linter,
        boundary: SourceBoundary<'a>,
        diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
        selection: &ProjectSelection,
        deadline: LoadDeadline,
    ) -> Result<Self, ProjectLoadError> {
        let session = linter.begin_project();
        let resolver = ProjectResolver::new(boundary.clone(), selection)?;
        let max_files = boundary.options().max_files();
        Ok(Self {
            session,
            resolver,
            boundary,
            diagnostics,
            queue: PathWorkQueue::default(),
            accepted: AcceptedPaths::new(max_files),
            resolved: ResolutionCache::default(),
            accepted_target_cache: BTreeMap::new(),
            sources: BTreeMap::new(),
            deadline,
        })
    }

    fn add_initial_paths(&mut self, paths: VecDeque<AcceptedSourcePath>) {
        self.queue.extend(paths);
    }

    /// Drain the work queue in bounded parallel waves and close the frontier.
    /// The result signals whether the frontier was fully drained or stopped by
    /// a recoverable error; successfully accepted and analyzed sources remain
    /// available on this state for partial report assembly.
    fn close_frontier(&mut self, metrics: &mut ProjectLoadMetrics) -> Result<(), ProjectLoadError> {
        let workers = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

        loop {
            self.deadline.check()?;

            let mut wave: Vec<AcceptedSourcePath> = Vec::with_capacity(WAVE_SIZE);
            while wave.len() < WAVE_SIZE {
                match self.queue.pop_front() {
                    Some(path) => wave.push(path),
                    None => break,
                }
            }

            if wave.is_empty() {
                return Ok(());
            }

            self.process_wave(&wave, workers, metrics)?;
        }
    }

    /// Admit, read, and locally analyze one bounded wave of source files in
    /// parallel, then resolve all emerging requests and enqueue internal
    /// targets for the next wave.
    ///
    /// When a budget check fails mid-wave (e.g. the project source-byte limit
    /// is hit), files that were successfully accepted and read are still
    /// submitted for parallel analysis so partial output is preserved.
    fn process_wave(
        &mut self,
        wave: &[AcceptedSourcePath],
        workers: NonZeroUsize,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        let read_start = Instant::now();
        let read = self.read_wave(wave, metrics)?;
        metrics.record_reads(read_start.elapsed());

        for source in &read.sources {
            self.sources
                .insert(source.path().clone(), source.source().clone());
        }

        // Analyze all sources collected so far in parallel, even if a later
        // file triggered a deferred budget error.
        if !read.sources.is_empty() {
            let parse_start = Instant::now();
            let requests = self.analyze_wave(read.sources, workers)?;
            metrics.record_analyze_source(parse_start.elapsed());
            metrics.record_files(self.accepted.len());

            metrics.admit_requests(requests.len(), self.boundary.options().max_requests())?;

            let (internal_targets, elapsed) = self.resolve_requests(requests)?;
            metrics.record_resolution(elapsed);
            self.apply_request_resolution(internal_targets, metrics)?;
        }

        // Propagate the deferred byte error after analyzed sources and their
        // request frontier transitions have been incorporated.
        if let Some(e) = read.deferred_error {
            return Err(e);
        }

        Ok(())
    }

    fn read_wave(
        &mut self,
        wave: &[AcceptedSourcePath],
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<ReadWaveOutcome, ProjectLoadError> {
        let source_limit = self.boundary.options().max_project_source_bytes();
        let mut sources = Vec::with_capacity(wave.len());
        let mut deferred_error = None;
        for accepted in wave {
            if !self.accepted.accept(accepted)? {
                continue;
            }

            // Check the cumulative byte budget against the on-disk size
            // before reading, so a file at the boundary is rejected without
            // wasting I/O.
            let md =
                std::fs::metadata(accepted.as_ref()).map_err(|source| ProjectLoadError::Io {
                    path: accepted.as_ref().to_path_buf(),
                    source,
                })?;
            if metrics.source_bytes().saturating_add(md.len()) > source_limit {
                deferred_error = Some(ProjectLoadError::ProjectSourceTooLarge {
                    bytes: metrics.source_bytes().saturating_add(md.len()),
                    limit: source_limit,
                });
                break;
            }

            let source = self.boundary.load_accepted_source_file(accepted)?;
            let source_bytes = u64::try_from(source.source().len())
                .unwrap_or_else(|_| source_limit.saturating_add(1));
            if let Err(error) = metrics.admit_source_bytes(source_bytes, source_limit) {
                deferred_error = Some(error);
                break;
            }
            sources.push(source);
        }
        Ok(ReadWaveOutcome {
            sources,
            deferred_error,
        })
    }

    fn analyze_wave(
        &mut self,
        sources: Vec<SourceFile>,
        workers: NonZeroUsize,
    ) -> Result<AuthoredRequests, ProjectLoadError> {
        self.session
            .analyze_sources(sources, workers)
            .map_err(ProjectLoadError::from)
    }

    fn resolve_requests(
        &mut self,
        requests: AuthoredRequests,
    ) -> Result<(Vec<ProjectRelativePath>, Duration), ProjectLoadError> {
        let mut internal_targets = Vec::new();
        let mut elapsed = Duration::ZERO;
        for request in requests {
            self.deadline.check()?;
            let resolve_start = Instant::now();
            let (result, resolved) = self.resolved.resolve_or_get(&request, &self.resolver)?;
            if resolved {
                elapsed += resolve_start.elapsed();
            }
            if let ResolverOutcome::Internal { path } = result {
                internal_targets.push(path.clone());
            }
        }
        Ok((internal_targets, elapsed))
    }

    fn apply_request_resolution(
        &mut self,
        internal_targets: Vec<ProjectRelativePath>,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        for path in internal_targets {
            self.enqueue_internal_target(path, metrics)?;
        }
        Ok(())
    }

    fn enqueue_internal_target(
        &mut self,
        path: glass_lint_core::project::ProjectRelativePath,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        metrics.record_edge();
        if let Some(accepted) = self.accepted_target_cache.get(&path) {
            self.queue.push(accepted.clone());
            return Ok(());
        }
        let target = self.boundary.canonical_root().join(&path);
        if target.exists()
            && let PathClassification::Accepted(accepted) = self.boundary.classify(&target)?
        {
            self.accepted_target_cache.insert(path, accepted.clone());
            self.queue.push(accepted);
        }
        Ok(())
    }

    fn finish(
        self,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(AnalysisReport, BTreeMap<ProjectRelativePath, SourceText>), ProjectLoadError> {
        let sources = self.sources;
        let (report, timings) = self.session.finish(self.resolved.into_iter())?.into_parts();
        metrics.record_linking(timings.linking());
        metrics.record_matching(timings.matching());
        let code = glass_lint_core::project::DiagnosticCode::new("tsconfig")
            .expect("tsconfig is a valid diagnostic code");
        let messages: Vec<String> = self
            .diagnostics
            .into_iter()
            .map(|diagnostic| {
                format!(
                    "{}: {}",
                    diagnostic.config_path.display(),
                    diagnostic.message
                )
            })
            .collect();
        let report = report.with_project_diagnostics(&code, messages);
        Ok((report, sources))
    }
}
