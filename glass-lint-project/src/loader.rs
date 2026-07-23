//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, VecDeque},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glass_lint_core::{
    Linter,
    project::{AnalysisReport, ResolutionRequest, ResolverOutcome},
};

use crate::{
    admission::{AdmissionSet, AdmittedSourcePath, SourceAdmission, absolute_path},
    discovery::{DiscoveryResult, ProjectDiscovery},
    error::ProjectLoadError,
    options::{ProjectSelection, ValidatedProjectLoadOptions},
    resolver::ProjectResolver,
    tsconfig,
};

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
    /// Recoverable boundary error that caused the partial report. Fatal
    /// errors, including timeout, are returned through the outer `Result`.
    pub partial_reason: Option<ProjectLoadError>,
    /// Phase timings and deterministic counters for this load.
    pub metrics: ProjectLoadMetrics,
}

impl ProjectLoadOutcome {
    fn complete(report: AnalysisReport) -> Self {
        Self {
            report,
            partial_reason: None,
            metrics: ProjectLoadMetrics::default(),
        }
    }

    fn partial(mut report: AnalysisReport, reason: ProjectLoadError) -> Self {
        let code = glass_lint_core::project::DiagnosticCode::new("incomplete_project")
            .expect("incomplete_project is a valid diagnostic code");
        report.completion = glass_lint_core::project::ReportCompletion::Partial;
        report
            .diagnostics
            .push(glass_lint_core::project::Diagnostic::Project(
                glass_lint_core::project::AnalysisDiagnostic {
                    code,
                    message: reason.to_string(),
                    location: None,
                },
            ));
        Self {
            report,
            partial_reason: Some(reason),
            metrics: ProjectLoadMetrics::default(),
        }
    }
}

/// Phase timings shared with harness profiling reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectPhaseTimings {
    discovery: Duration,
    reads: Duration,
    analyze_source: Duration,
    resolution: Duration,
    linking: Duration,
    matching: Duration,
    total: Duration,
}

impl ProjectPhaseTimings {
    pub fn with_discovery(duration: Duration) -> Self {
        let mut timings = Self::default();
        timings.record_discovery(duration);
        timings
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

impl std::ops::AddAssign for ProjectPhaseTimings {
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

/// Bounded construction counters and phase timings for profiling.
///
/// Embeds [`ProjectPhaseTimings`] directly so that the duration fields have
/// one authoritative representation across timings, metrics, and phase-timing
/// conversions.
#[derive(Clone, Debug, Default)]
pub struct ProjectLoadMetrics {
    /// Phase durations embedded directly as the canonical timing record.
    pub timings: ProjectPhaseTimings,
    /// Number of admitted source files.
    pub files: usize,
    /// Number of resolver requests observed.
    pub requests: usize,
    /// Number of internal edges observed.
    pub edges: usize,
    /// Total source bytes read.
    pub bytes: u64,
}

impl ProjectLoadMetrics {
    #[must_use]
    pub fn phase_timings(&self) -> ProjectPhaseTimings {
        self.timings
    }
}

impl std::ops::AddAssign for ProjectLoadMetrics {
    fn add_assign(&mut self, rhs: Self) {
        self.timings += rhs.timings;
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.bytes = self.bytes.saturating_add(rhs.bytes);
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
        metrics.timings.record_total(total_start.elapsed());
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
        let deadline = Instant::now() + Duration::from_millis(self.options.max_timeout_ms());
        let paths = ProjectPaths::from_selection(&self.options, selection, deadline)?;
        metrics.timings.record_discovery(discovery_start.elapsed());

        let mut build = ProjectLoadState::new(
            linter,
            paths.admission,
            paths.diagnostics,
            selection,
            deadline,
        )?;
        build.add_initial_paths(paths.initial_paths);
        let (expansion_result, closed) = build.close_frontier(metrics);
        match expansion_result {
            Ok(()) => Ok(ProjectLoadOutcome::complete(closed.finish(metrics)?)),
            Err(ProjectLoadError::Timeout) => Err(ProjectLoadError::Timeout),
            Err(error) => {
                let report = closed.finish_partial(metrics)?;
                Ok(ProjectLoadOutcome::partial(report, error))
            }
        }
    }
}

/// Canonical absolute paths established before the load loop starts.
struct ProjectPaths<'a> {
    admission: SourceAdmission<'a>,
    initial_paths: VecDeque<AdmittedSourcePath>,
    diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
}

impl<'a> ProjectPaths<'a> {
    fn from_selection(
        options: &'a ValidatedProjectLoadOptions,
        selection: &ProjectSelection,
        deadline: Instant,
    ) -> Result<Self, ProjectLoadError> {
        let selection_path = absolute_path(selection.path())?;
        if !selection_path.exists() {
            return Err(ProjectLoadError::SelectionNotFound(selection_path));
        }
        let root = project_root(options, selection, &selection_path)?;
        let admission = SourceAdmission::new(&root, options)?;
        let canonical_selection = SourceAdmission::canonicalize(&selection_path)?;
        if !admission.is_inside_root(canonical_selection.as_ref()) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: canonical_selection.into_path_buf(),
                root,
            });
        }
        let discover = ProjectDiscovery::with_deadline(
            &admission,
            deadline,
            options.max_files(),
            tsconfig::ConfigTraversalBudget::new(
                options.max_config_count(),
                options.max_config_depth(),
            ),
        );
        let DiscoveryResult { paths, diagnostics } =
            discover.initial_paths(selection, canonical_selection.as_ref())?;
        Ok(Self {
            admission,
            initial_paths: paths.into(),
            diagnostics,
        })
    }
}

#[derive(Default)]
struct PathWorkQueue(VecDeque<AdmittedSourcePath>);
impl PathWorkQueue {
    fn extend(&mut self, paths: impl IntoIterator<Item = AdmittedSourcePath>) {
        self.0.extend(paths);
    }

    fn pop_front(&mut self) -> Option<AdmittedSourcePath> {
        self.0.pop_front()
    }

    fn push(&mut self, path: AdmittedSourcePath) {
        self.0.push_back(path);
    }
}

#[derive(Debug, Default)]
struct ResolutionCache(BTreeMap<glass_lint_core::project::ResolutionRequestKey, ResolverOutcome>);
impl ResolutionCache {
    /// Resolve a request if not already cached and return the stored outcome.
    /// The returned `bool` is `true` when a real resolution was performed.
    fn resolve_or_get(
        &mut self,
        request: &ResolutionRequest,
        resolver: &ProjectResolver,
    ) -> Result<(&ResolverOutcome, bool), ProjectLoadError> {
        let cache_key = request.key.clone();
        match self.0.entry(cache_key) {
            std::collections::btree_map::Entry::Occupied(e) => Ok((e.into_mut(), false)),
            std::collections::btree_map::Entry::Vacant(e) => {
                Ok((e.insert(resolver.resolve(request)?), true))
            }
        }
    }

    fn into_iter(
        self,
    ) -> impl Iterator<
        Item = (
            glass_lint_core::project::ResolutionRequestKey,
            ResolverOutcome,
        ),
    > {
        self.0.into_iter()
    }
}

#[derive(Debug, Default)]
struct LoadProgress {
    requests: usize,
    edges: usize,
    source_bytes: u64,
}

impl LoadProgress {
    fn add_requests(&mut self, count: usize, limit: usize) -> Result<(), ProjectLoadError> {
        self.requests = self
            .requests
            .checked_add(count)
            .ok_or(ProjectLoadError::TooManyRequests(limit))?;
        if self.requests > limit {
            return Err(ProjectLoadError::TooManyRequests(limit));
        }
        Ok(())
    }

    fn record_edge(&mut self) {
        self.edges = self.edges.saturating_add(1);
    }

    fn record_source_bytes(&mut self, bytes: u64, limit: u64) -> Result<(), ProjectLoadError> {
        self.source_bytes = self.source_bytes.saturating_add(bytes);
        if self.source_bytes > limit {
            return Err(ProjectLoadError::ProjectSourceTooLarge {
                bytes: self.source_bytes,
                limit,
            });
        }
        Ok(())
    }

    fn publish(&self, metrics: &mut ProjectLoadMetrics) {
        metrics.requests = self.requests;
        metrics.edges = self.edges;
        metrics.bytes = self.source_bytes;
    }
}

/// Maximum number of files processed in one parallel wave. Independent of
/// the total file limit so that parallelism does not create an unbounded
/// memory spike.
const WAVE_SIZE: usize = 50;

/// Mutable state for one project construction. Keeping the queue, cache, and
/// counters together makes the main loading phases explicit and auditable.
struct ProjectLoadState<'a> {
    session: glass_lint_core::project::ProjectCollection<'a>,
    resolver: ProjectResolver<'a>,
    admission: SourceAdmission<'a>,
    diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
    queue: PathWorkQueue,
    admitted: AdmissionSet,
    resolved: ResolutionCache,
    progress: LoadProgress,
    deadline: Instant,
}

impl<'a> ProjectLoadState<'a> {
    fn new(
        linter: &'a Linter,
        admission: SourceAdmission<'a>,
        diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
        selection: &ProjectSelection,
        deadline: Instant,
    ) -> Result<Self, ProjectLoadError> {
        let session = linter.begin_project(admission.canonical_root())?;
        let resolver = ProjectResolver::new(admission.clone(), selection)?;
        let max_files = admission.options().max_files();
        Ok(Self {
            session,
            resolver,
            admission,
            diagnostics,
            queue: PathWorkQueue::default(),
            admitted: AdmissionSet::new(max_files),
            resolved: ResolutionCache::default(),
            progress: LoadProgress::default(),
            deadline,
        })
    }

    fn add_initial_paths(&mut self, paths: VecDeque<AdmittedSourcePath>) {
        self.queue.extend(paths);
    }

    /// Drain the work queue in bounded parallel waves and close the frontier,
    /// returning a typed [`ClosedFrontier`] that can only be used for linking
    /// and matching.  Frontier expansion and report generation are now visibly
    /// separate phases. The result signals whether the frontier was fully
    /// drained or stopped by a recoverable error; the `ClosedFrontier` is
    /// always produced so callers can still assemble a partial report.
    fn close_frontier(
        mut self,
        metrics: &mut ProjectLoadMetrics,
    ) -> (Result<(), ProjectLoadError>, ClosedFrontier<'a>) {
        let workers = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

        let result = loop {
            if let Err(e) = self.check_timeout() {
                break Err(e);
            }

            let mut wave: Vec<AdmittedSourcePath> = Vec::with_capacity(WAVE_SIZE);
            while wave.len() < WAVE_SIZE {
                match self.queue.pop_front() {
                    Some(path) => wave.push(path),
                    None => break,
                }
            }

            if wave.is_empty() {
                break Ok(());
            }

            if let Err(e) = self.process_wave(&wave, workers, metrics) {
                break Err(e);
            }
        };
        let frontier = ClosedFrontier {
            session: self.session,
            resolved: self.resolved,
            diagnostics: self.diagnostics,
            deadline: self.deadline,
        };
        (result, frontier)
    }

    /// Admit, read, and locally analyze one bounded wave of source files in
    /// parallel, then resolve all emerging requests and enqueue internal
    /// targets for the next wave.
    ///
    /// When a budget check fails mid-wave (e.g. the project source-byte limit
    /// is hit), files that were successfully admitted and read are still
    /// submitted for parallel analysis so partial output is preserved.
    fn process_wave(
        &mut self,
        wave: &[AdmittedSourcePath],
        workers: NonZeroUsize,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        let source_limit = self.admission.options().max_project_source_bytes();

        // Phase 1: admit and read every source in the wave.  If a cumulative
        // budget is exceeded, we defer the error and analyse whatever we
        // already read, preserving partial-report semantics.
        let read_start = Instant::now();
        let mut sources = Vec::with_capacity(wave.len());
        let mut byte_error = None;
        for admitted in wave {
            if !self.admitted.admit(admitted)? {
                continue;
            }
            let source = self.admission.load_admitted_source_file(admitted)?;
            let source_bytes = u64::try_from(source.source().len())
                .unwrap_or_else(|_| source_limit.saturating_add(1));
            if let Err(e) = self
                .progress
                .record_source_bytes(source_bytes, source_limit)
            {
                byte_error = Some(e);
                break;
            }
            sources.push(source);
        }
        metrics.timings.record_reads(read_start.elapsed());

        // Phase 2: analyze all sources collected so far in parallel, even if
        // a later file triggered a budget error.
        if !sources.is_empty() {
            let parse_start = Instant::now();
            let requests = self.session.analyze_sources(sources, workers)?;
            metrics.timings.record_analyze_source(parse_start.elapsed());
            metrics.files = self.admitted.len();

            self.progress
                .add_requests(requests.len(), self.admission.options().max_requests())?;
            self.progress.publish(metrics);
            self.record_requests(requests, metrics)?;
        }

        // Phase 3: propagate the deferred byte error after the analysed files
        // have been incorporated.
        if let Some(e) = byte_error {
            return Err(e);
        }

        Ok(())
    }

    fn record_requests(
        &mut self,
        requests: Vec<ResolutionRequest>,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        for request in requests {
            self.check_timeout()?;
            let resolve_start = Instant::now();
            let (result, resolved) = self.resolved.resolve_or_get(&request, &self.resolver)?;
            if resolved {
                metrics.timings.record_resolution(resolve_start.elapsed());
            }
            let internal_target = match result {
                ResolverOutcome::Internal { path } => Some(path.clone()),
                _ => None,
            };
            self.enqueue_internal_target(internal_target, metrics)?;
        }
        Ok(())
    }

    fn check_timeout(&self) -> Result<(), ProjectLoadError> {
        (Instant::now() <= self.deadline)
            .then_some(())
            .ok_or(ProjectLoadError::Timeout)
    }

    fn enqueue_internal_target(
        &mut self,
        path: Option<glass_lint_core::project::ProjectRelativePath>,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        if let Some(path) = path {
            self.progress.record_edge();
            self.progress.publish(metrics);
            let target = self.admission.canonical_root().join(path);
            if target.exists()
                && let crate::admission::PathAdmission::Admitted(admitted) =
                    self.admission.classify(&target)?
            {
                self.queue.push(admitted);
            }
        }
        Ok(())
    }
}

/// The closed project frontier after the work queue has been fully drained.
/// Frontier expansion (file reading, local analysis, resolution) is complete;
/// the only remaining transition is linking and matching.
struct ClosedFrontier<'a> {
    session: glass_lint_core::project::ProjectCollection<'a>,
    resolved: ResolutionCache,
    diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
    deadline: Instant,
}

impl ClosedFrontier<'_> {
    fn check_timeout(&self) -> Result<(), ProjectLoadError> {
        (Instant::now() <= self.deadline)
            .then_some(())
            .ok_or(ProjectLoadError::Timeout)
    }

    fn finish(self, metrics: &mut ProjectLoadMetrics) -> Result<AnalysisReport, ProjectLoadError> {
        self.check_timeout()?;
        self.finish_inner(metrics)
    }

    fn finish_partial(
        self,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<AnalysisReport, ProjectLoadError> {
        self.finish_inner(metrics)
    }

    fn finish_inner(
        self,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<AnalysisReport, ProjectLoadError> {
        let local = self.session.finish_local();
        let resolved = local.resolve(self.resolved.into_iter())?;
        let result = resolved.finish_with_timings()?;
        metrics.timings.record_linking(result.linking);
        metrics.timings.record_matching(result.matching);
        let mut report = result.report;
        let code = glass_lint_core::project::DiagnosticCode::new("tsconfig")
            .expect("tsconfig is a valid diagnostic code");
        report
            .diagnostics
            .extend(self.diagnostics.into_iter().map(|diagnostic| {
                glass_lint_core::project::Diagnostic::Project(
                    glass_lint_core::project::AnalysisDiagnostic {
                        code: code.clone(),
                        message: format!(
                            "{}: {}",
                            diagnostic.config_path.display(),
                            diagnostic.message
                        ),
                        location: None,
                    },
                )
            }));
        Ok(report)
    }
}

fn project_root(
    options: &ValidatedProjectLoadOptions,
    selection: &ProjectSelection,
    path: &Path,
) -> Result<PathBuf, ProjectLoadError> {
    if let Some(root) = options.root() {
        return absolute_path(root);
    }
    Ok(match selection {
        ProjectSelection::Directory(_) => path.to_path_buf(),
        ProjectSelection::Entry(_) | ProjectSelection::Tsconfig(_) => {
            path.parent().unwrap_or(path).to_path_buf()
        }
    })
}
