//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glass_lint_core::{
    AnalysisReport, Linter, ProjectRelativePath, ResolutionRequest, ResolutionRequestKind,
    ResolverOutcome,
};

use crate::{
    discovery::{ProjectDiscovery, absolute_path, inside_root, realpath},
    error::ProjectLoadError,
    options::{ProjectLoadOptions, ProjectSelection, ValidatedProjectLoadOptions},
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

    fn partial(
        mut report: AnalysisReport,
        reason: ProjectLoadError,
    ) -> Result<Self, ProjectLoadError> {
        let code = glass_lint_core::DiagnosticCode::new("incomplete_project").map_err(|error| {
            ProjectLoadError::InvalidOptions(crate::ProjectOptionError::Message(error))
        })?;
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
        Ok(Self {
            report,
            partial_reason: Some(reason),
            metrics: ProjectLoadMetrics::default(),
        })
    }
}

/// Shared timing phase fields for loading and profiling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ProjectPhases {
    discovery: Duration,
    reads: Duration,
    parse_and_local_analysis: Duration,
    resolution: Duration,
    linking_and_matching: Duration,
    linking: Duration,
    matching: Duration,
    total: Duration,
}

impl std::ops::AddAssign for ProjectPhases {
    fn add_assign(&mut self, rhs: Self) {
        self.discovery = self.discovery.saturating_add(rhs.discovery);
        self.reads = self.reads.saturating_add(rhs.reads);
        self.parse_and_local_analysis = self
            .parse_and_local_analysis
            .saturating_add(rhs.parse_and_local_analysis);
        self.resolution = self.resolution.saturating_add(rhs.resolution);
        self.linking_and_matching = self
            .linking_and_matching
            .saturating_add(rhs.linking_and_matching);
        self.linking = self.linking.saturating_add(rhs.linking);
        self.matching = self.matching.saturating_add(rhs.matching);
        self.total = self.total.saturating_add(rhs.total);
    }
}

/// Bounded construction counters and phase timings for profiling.
///
/// Loader-owned discovery, reads, parsing, and resolution are separated from
/// core-reported linking/matching phases; `total` covers the complete load
/// operation.
#[derive(Clone, Debug, Default)]
pub struct ProjectLoadMetrics {
    /// Time spent selecting and discovering files.
    pub discovery: Duration,
    /// Time spent reading source bytes.
    pub reads: Duration,
    /// Time spent parsing and locally analyzing sources.
    pub parse_and_local_analysis: Duration,
    /// Time spent resolving module requests.
    pub resolution: Duration,
    /// End-to-end linking and matching time at the core boundary.
    pub linking_and_matching: Duration,
    /// Link-only phase reported by core.
    pub linking: Duration,
    /// Matcher-only phase reported by core.
    pub matching: Duration,
    /// Total elapsed load time.
    pub total: Duration,
    /// Number of admitted source files.
    pub files: usize,
    /// Number of resolver requests observed.
    pub requests: usize,
    /// Number of internal edges observed.
    pub edges: usize,
    /// Total source bytes read.
    pub bytes: u64,
}

/// Phase timings shared with harness profiling reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectPhaseTimings {
    pub discovery: Duration,
    pub reads: Duration,
    pub parse_and_local_analysis: Duration,
    pub resolution: Duration,
    pub linking: Duration,
    pub linking_and_matching: Duration,
    pub matching: Duration,
    pub total: Duration,
}

impl ProjectPhaseTimings {
    fn phases(&self) -> ProjectPhases {
        ProjectPhases {
            discovery: self.discovery,
            reads: self.reads,
            parse_and_local_analysis: self.parse_and_local_analysis,
            resolution: self.resolution,
            linking: self.linking,
            linking_and_matching: self.linking_and_matching,
            matching: self.matching,
            total: self.total,
        }
    }
}

impl std::ops::AddAssign for ProjectPhaseTimings {
    fn add_assign(&mut self, rhs: Self) {
        let mut phases = self.phases();
        phases += rhs.phases();
        self.discovery = phases.discovery;
        self.reads = phases.reads;
        self.parse_and_local_analysis = phases.parse_and_local_analysis;
        self.resolution = phases.resolution;
        self.linking = phases.linking;
        self.linking_and_matching = phases.linking_and_matching;
        self.matching = phases.matching;
        self.total = phases.total;
    }
}

impl ProjectLoadMetrics {
    fn phases(&self) -> ProjectPhases {
        ProjectPhases {
            discovery: self.discovery,
            reads: self.reads,
            parse_and_local_analysis: self.parse_and_local_analysis,
            resolution: self.resolution,
            linking: self.linking,
            linking_and_matching: self.linking_and_matching,
            matching: self.matching,
            total: self.total,
        }
    }

    #[must_use]
    pub fn phase_timings(&self) -> ProjectPhaseTimings {
        ProjectPhaseTimings {
            discovery: self.discovery,
            reads: self.reads,
            parse_and_local_analysis: self.parse_and_local_analysis,
            resolution: self.resolution,
            linking: self.linking,
            linking_and_matching: self.linking_and_matching,
            matching: self.matching,
            total: self.total,
        }
    }
}

impl std::ops::AddAssign for ProjectLoadMetrics {
    fn add_assign(&mut self, rhs: Self) {
        let mut phases = self.phases();
        phases += rhs.phases();
        self.discovery = phases.discovery;
        self.reads = phases.reads;
        self.parse_and_local_analysis = phases.parse_and_local_analysis;
        self.resolution = phases.resolution;
        self.linking_and_matching = phases.linking_and_matching;
        self.linking = phases.linking;
        self.matching = phases.matching;
        self.total = phases.total;
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

    /// Borrow the validated policy used by this loader.
    pub fn options(&self) -> &ProjectLoadOptions {
        &self.options
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
        metrics.total = total_start.elapsed();
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
        let deadline = Instant::now() + Duration::from_millis(self.options.max_timeout_ms);
        let paths = ProjectPaths::from_selection(&self.options, selection, deadline)?;
        metrics.discovery += discovery_start.elapsed();

        let mut build =
            ProjectLoadState::new(linter, &self.options, paths.root, selection, deadline)?;
        build.add_initial_paths(paths.initial_paths);
        match build.load_all(metrics) {
            Ok(()) => Ok(ProjectLoadOutcome::complete(build.finish(metrics)?)),
            Err(ProjectLoadError::Timeout) => Err(ProjectLoadError::Timeout),
            Err(error) => {
                let report = build.finish_partial(metrics)?;
                ProjectLoadOutcome::partial(report, error)
            }
        }
    }
}

/// Canonical absolute paths established before the load loop starts.
struct ProjectPaths {
    root: PathBuf,
    initial_paths: VecDeque<PathBuf>,
}

impl ProjectPaths {
    fn from_selection(
        options: &ProjectLoadOptions,
        selection: &ProjectSelection,
        deadline: Instant,
    ) -> Result<Self, ProjectLoadError> {
        let selection_path = absolute_path(selection.path());
        if !selection_path.exists() {
            return Err(ProjectLoadError::SelectionNotFound(selection_path));
        }
        let selection_path = realpath(&selection_path)?;
        let root = realpath(&project_root(options, selection, &selection_path))?;
        if !inside_root(&root, &selection_path) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: selection_path,
                root,
            });
        }
        let initial_paths = ProjectDiscovery::with_deadline(options, deadline).initial_paths(
            selection,
            &selection_path,
            &root,
        )?;
        Ok(Self {
            root,
            initial_paths: initial_paths.into(),
        })
    }
}

#[derive(Default)]
struct PathWorkQueue(VecDeque<PathBuf>);
impl PathWorkQueue {
    fn extend(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.0.extend(paths);
    }

    fn pop_front(&mut self) -> Option<PathBuf> {
        self.0.pop_front()
    }

    fn push(&mut self, path: PathBuf) {
        self.0.push_back(path);
    }
}

#[derive(Debug, Default)]
struct AdmissionSet(BTreeSet<PathBuf>);
impl AdmissionSet {
    fn admit(&mut self, path: PathBuf) -> bool {
        self.0.insert(path)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ResolutionCacheKey {
    importer: ProjectRelativePath,
    kind: ResolutionRequestKind,
    request: String,
}

impl ResolutionCacheKey {
    fn new(importer: ProjectRelativePath, kind: ResolutionRequestKind, request: String) -> Self {
        Self {
            importer,
            kind,
            request,
        }
    }
}

#[derive(Debug, Default)]
struct ResolutionCache(BTreeMap<ResolutionCacheKey, ResolverOutcome>);
impl ResolutionCache {
    fn get(&self, key: &ResolutionCacheKey) -> Option<&ResolverOutcome> {
        self.0.get(key)
    }

    fn insert(&mut self, key: ResolutionCacheKey, result: ResolverOutcome) {
        self.0.insert(key, result);
    }
}

#[derive(Debug, Default)]
struct LoadCounters {
    requests: usize,
    edges: usize,
    source_bytes: u64,
}

impl LoadCounters {
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
}

/// Mutable state for one project construction. Keeping the queue, cache, and
/// counters together makes the main loading phases explicit and auditable.
struct ProjectLoadState<'a> {
    session: glass_lint_core::AnalysisSession<'a>,
    discovery: ProjectDiscovery<'a>,
    resolver: ProjectResolver,
    root: PathBuf,
    queue: PathWorkQueue,
    admitted: AdmissionSet,
    resolved: ResolutionCache,
    counters: LoadCounters,
    deadline: Instant,
}

impl<'a> ProjectLoadState<'a> {
    fn new(
        linter: &'a Linter,
        options: &'a ProjectLoadOptions,
        root: PathBuf,
        selection: &ProjectSelection,
        deadline: Instant,
    ) -> Result<Self, ProjectLoadError> {
        Ok(Self {
            session: linter.begin_analysis(&root)?,
            discovery: ProjectDiscovery::with_deadline(options, deadline),
            resolver: ProjectResolver::new(&root, selection, options),
            root,
            queue: PathWorkQueue::default(),
            admitted: AdmissionSet::default(),
            resolved: ResolutionCache::default(),
            counters: LoadCounters::default(),
            deadline,
        })
    }

    fn add_initial_paths(&mut self, paths: VecDeque<PathBuf>) {
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
        path: &Path,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        self.check_timeout()?;
        let path = realpath(path)?;
        if !inside_root(&self.root, &path) || !self.admitted.admit(path.clone()) {
            return Ok(());
        }
        if self.admitted.len() > self.discovery.options().max_files {
            return Err(ProjectLoadError::TooManyFiles(
                self.discovery.options().max_files,
            ));
        }

        let read_start = Instant::now();
        let source = self.discovery.read_source(&self.root, &path)?;
        metrics.reads += read_start.elapsed();
        let source_bytes = u64::try_from(source.source.len()).unwrap_or(u64::MAX);
        self.counters.source_bytes = self.counters.source_bytes.saturating_add(source_bytes);
        if self.counters.source_bytes > self.discovery.options().max_project_source_bytes {
            return Err(ProjectLoadError::ProjectSourceTooLarge {
                bytes: self.counters.source_bytes,
                limit: self.discovery.options().max_project_source_bytes,
            });
        }

        let parse_start = Instant::now();
        let source_path = source.path.to_string();
        self.session.admit_source(source)?;
        let requests = self.session.analyze_source(source_path)?;
        metrics.parse_and_local_analysis += parse_start.elapsed();
        metrics.bytes = metrics.bytes.saturating_add(source_bytes);
        metrics.files = self.admitted.len();

        self.counters
            .add_requests(requests.len(), self.discovery.options().max_requests)?;
        metrics.requests = self.counters.requests;
        self.record_requests(requests, metrics)
    }

    fn record_requests(
        &mut self,
        requests: Vec<ResolutionRequest>,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
        for request in requests {
            self.check_timeout()?;
            let cache_key = ResolutionCacheKey::new(
                request.key.importer.clone(),
                request.key.kind,
                request.request.clone(),
            );
            let result = if let Some(result) = self.resolved.get(&cache_key) {
                result.clone()
            } else {
                let resolve_start = Instant::now();
                let result = self.resolver.resolve(&request);
                metrics.resolution += resolve_start.elapsed();
                self.resolved.insert(cache_key, result.clone());
                result
            };
            self.enqueue_internal_target(&result, metrics);
            self.session.record_resolution(request.key, result)?;
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
        result: &ResolverOutcome,
        metrics: &mut ProjectLoadMetrics,
    ) {
        if let ResolverOutcome::Internal { path } = result {
            self.counters.record_edge();
            metrics.edges = self.counters.edges;
            let target = self.root.join(path);
            if target.exists()
                && !self
                    .discovery
                    .options()
                    .excludes_path(&self.root, &target)
                && self.discovery.options().supports(&target)
            {
                self.queue.push(target);
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
        let link_start = Instant::now();
        let (report, linking, matching) = self.session.finish_with_timings()?;
        if Instant::now() > deadline {
            return Err(ProjectLoadError::Timeout);
        }
        metrics.linking += linking;
        metrics.matching += matching;
        metrics.linking_and_matching += link_start.elapsed();
        Ok(report)
    }
}

fn project_root(
    options: &ProjectLoadOptions,
    selection: &ProjectSelection,
    path: &Path,
) -> PathBuf {
    if let Some(root) = &options.root {
        return absolute_path(root);
    }
    match selection {
        ProjectSelection::Directory(_) => path.to_path_buf(),
        ProjectSelection::Entry(_) | ProjectSelection::Tsconfig(_) => {
            path.parent().unwrap_or(path).to_path_buf()
        }
    }
}
