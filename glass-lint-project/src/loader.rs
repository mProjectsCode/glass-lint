//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glass_lint_core::{AnalysisReport, Linter, ResolutionRequest, ResolverOutcome};

use crate::{
    admission::{CanonicalProjectPath, SourceAdmission, absolute_path},
    discovery::ProjectDiscovery,
    error::ProjectLoadError,
    options::{ProjectSelection, ValidatedProjectLoadOptions},
    resolver::ProjectResolver,
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
        let code = glass_lint_core::DiagnosticCode::new("incomplete_project")
            .expect("incomplete_project is a valid diagnostic code");
        report.completion = glass_lint_core::ReportCompletion::Partial;
        report
            .diagnostics
            .push(glass_lint_core::Diagnostic::Project(
                glass_lint_core::AnalysisDiagnostic {
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
    local_analysis: Duration,
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
    pub fn local_analysis(&self) -> Duration {
        self.local_analysis
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
        self.local_analysis
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

    pub fn record_local_analysis(&mut self, duration: Duration) {
        self.local_analysis = self.local_analysis.saturating_add(duration);
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
        self.local_analysis = self.local_analysis.saturating_add(rhs.local_analysis);
        self.resolution = self.resolution.saturating_add(rhs.resolution);
        self.linking = self.linking.saturating_add(rhs.linking);
        self.matching = self.matching.saturating_add(rhs.matching);
        self.total = self.total.saturating_add(rhs.total);
    }
}

/// Bounded construction counters and phase timings for profiling.
///
/// Embeds [`ProjectPhaseTimings`] directly so that the eight duration fields
/// have one authoritative representation across timings, metrics, and
/// phase-timing conversions.
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

        let mut build = ProjectLoadState::new(linter, paths.admission, selection, deadline)?;
        build.add_initial_paths(paths.initial_paths);
        match build.load_all(metrics) {
            Ok(()) => Ok(ProjectLoadOutcome::complete(build.finish(metrics)?)),
            Err(ProjectLoadError::Timeout) => Err(ProjectLoadError::Timeout),
            Err(error) => {
                let report = build.finish_partial(metrics)?;
                Ok(ProjectLoadOutcome::partial(report, error))
            }
        }
    }
}

/// Canonical absolute paths established before the load loop starts.
struct ProjectPaths<'a> {
    admission: SourceAdmission<'a>,
    initial_paths: VecDeque<CanonicalProjectPath>,
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
        let canonical_selection = admission.canonicalize(&selection_path)?;
        if !admission.is_inside_root(canonical_selection.as_ref()) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: canonical_selection.into_path_buf(),
                root,
            });
        }
        let initial_paths: VecDeque<CanonicalProjectPath> =
            ProjectDiscovery::with_deadline(&admission, deadline)
                .initial_paths(selection, canonical_selection.as_ref())?
                .into();
        Ok(Self {
            admission,
            initial_paths,
        })
    }
}

#[derive(Default)]
struct PathWorkQueue(VecDeque<CanonicalProjectPath>);
impl PathWorkQueue {
    fn extend(&mut self, paths: impl IntoIterator<Item = CanonicalProjectPath>) {
        self.0.extend(paths);
    }

    fn pop_front(&mut self) -> Option<CanonicalProjectPath> {
        self.0.pop_front()
    }

    fn push(&mut self, path: CanonicalProjectPath) {
        self.0.push_back(path);
    }
}

#[derive(Debug, Default)]
struct AdmissionSet(BTreeSet<CanonicalProjectPath>);
impl AdmissionSet {
    fn admit(&mut self, path: CanonicalProjectPath) -> bool {
        self.0.insert(path)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Default)]
struct ResolutionCache(BTreeMap<glass_lint_core::ResolutionRequestKey, ResolverOutcome>);
impl ResolutionCache {
    fn get(&self, key: &glass_lint_core::ResolutionRequestKey) -> Option<&ResolverOutcome> {
        self.0.get(key)
    }

    fn insert(&mut self, key: glass_lint_core::ResolutionRequestKey, result: ResolverOutcome) {
        self.0.insert(key, result);
    }

    fn into_iter(
        self,
    ) -> impl Iterator<Item = (glass_lint_core::ResolutionRequestKey, ResolverOutcome)> {
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

/// Mutable state for one project construction. Keeping the queue, cache, and
/// counters together makes the main loading phases explicit and auditable.
struct ProjectLoadState<'a> {
    session: glass_lint_core::ProjectCollection<'a>,
    resolver: ProjectResolver<'a>,
    admission: SourceAdmission<'a>,
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
        selection: &ProjectSelection,
        deadline: Instant,
    ) -> Result<Self, ProjectLoadError> {
        let session = linter.begin_project(admission.canonical_root())?;
        let resolver = ProjectResolver::new(admission.clone(), selection)?;
        Ok(Self {
            session,
            resolver,
            admission,
            queue: PathWorkQueue::default(),
            admitted: AdmissionSet::default(),
            resolved: ResolutionCache::default(),
            progress: LoadProgress::default(),
            deadline,
        })
    }

    fn add_initial_paths(&mut self, paths: VecDeque<CanonicalProjectPath>) {
        self.queue.extend(paths);
    }

    fn load_all(&mut self, metrics: &mut ProjectLoadMetrics) -> Result<(), ProjectLoadError> {
        while let Some(path) = self.queue.pop_front() {
            self.check_timeout()?;
            self.load_path(&path, metrics)?;
        }
        Ok(())
    }

    fn load_path(
        &mut self,
        canonical: &CanonicalProjectPath,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        self.check_timeout()?;
        let Some(admitted) = self.admission.admitted_path(canonical.as_ref())? else {
            return Ok(());
        };
        if !self.admitted.admit(canonical.clone()) {
            return Ok(());
        }
        if self.admitted.len() > self.admission.options().max_files() {
            return Err(ProjectLoadError::TooManyFiles(
                self.admission.options().max_files(),
            ));
        }

        let read_start = Instant::now();
        let source = self.admission.load_admitted_source_file(&admitted)?;
        metrics.timings.record_reads(read_start.elapsed());
        let source_bytes = u64::try_from(source.source.len()).unwrap_or(u64::MAX);
        self.progress.record_source_bytes(
            source_bytes,
            self.admission.options().max_project_source_bytes(),
        )?;

        let parse_start = Instant::now();
        let requests = self.session.analyze_source(source)?.requests();
        metrics.timings.record_local_analysis(parse_start.elapsed());
        metrics.files = self.admitted.len();

        self.progress
            .add_requests(requests.len(), self.admission.options().max_requests())?;
        self.progress.publish(metrics);
        self.record_requests(requests, metrics)
    }

    fn record_requests(
        &mut self,
        requests: Vec<ResolutionRequest>,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        for request in requests {
            self.check_timeout()?;
            let cache_key = request.key.clone();
            if self.resolved.get(&cache_key).is_none() {
                let resolve_start = Instant::now();
                let result = self.resolver.resolve(&request);
                metrics.timings.record_resolution(resolve_start.elapsed());
                self.resolved.insert(cache_key.clone(), result);
            }
            let result = self
                .resolved
                .get(&cache_key)
                .expect("resolution was inserted");
            let internal_target = match result {
                ResolverOutcome::Internal { path } => Some(path.clone()),
                _ => None,
            };
            self.enqueue_internal_target(internal_target, metrics);
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
        path: Option<glass_lint_core::ProjectRelativePath>,
        metrics: &mut ProjectLoadMetrics,
    ) {
        if let Some(path) = path {
            self.progress.record_edge();
            self.progress.publish(metrics);
            let target = self.admission.canonical_root().join(path);
            if target.exists()
                && let Ok(canonical) = self.admission.canonicalize(&target)
            {
                self.queue.push(canonical);
            }
        }
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
        let deadline = self.deadline;
        let local = self.session.finish_local();
        let resolved = local.resolve(self.resolved.into_iter())?;
        let (report, linking, matching) = resolved.finish_with_timings()?;
        if Instant::now() > deadline {
            return Err(ProjectLoadError::Timeout);
        }
        metrics.timings.record_linking(linking);
        metrics.timings.record_matching(matching);
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
