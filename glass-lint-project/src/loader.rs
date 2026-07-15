//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glass_lint_core::{
    Linter, ProjectReport, ResolutionRequest, ResolutionRequestKind, ResolutionResult,
};

use crate::{
    discovery::{ProjectDiscovery, absolute_path, inside_root, realpath},
    error::ProjectLoadError,
    options::{ProjectLoadOptions, ProjectSelection},
    resolver::ProjectResolver,
};

/// Filesystem loader and Oxc resolver configuration.
#[derive(Clone, Debug)]
pub struct ProjectLoader {
    options: ProjectLoadOptions,
}

/// Bounded construction counters and phase timings for profiling. The
/// timings intentionally stop at the core boundary; matcher work is included
/// in `linking_and_matching` because core owns the completed project pass.
#[derive(Clone, Debug, Default)]
pub struct ProjectLoadMetrics {
    pub discovery: Duration,
    pub reads: Duration,
    pub parse_and_local_analysis: Duration,
    pub resolution: Duration,
    pub linking_and_matching: Duration,
    pub linking: Duration,
    pub matching: Duration,
    pub total: Duration,
    pub files: usize,
    pub requests: usize,
    pub edges: usize,
    pub bytes: u64,
}

impl std::ops::AddAssign for ProjectLoadMetrics {
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
        self.files = self.files.saturating_add(rhs.files);
        self.requests = self.requests.saturating_add(rhs.requests);
        self.edges = self.edges.saturating_add(rhs.edges);
        self.bytes = self.bytes.saturating_add(rhs.bytes);
    }
}

impl ProjectLoader {
    pub fn new(options: ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        options.validate()?;
        Ok(Self { options })
    }

    pub fn options(&self) -> &ProjectLoadOptions {
        &self.options
    }

    /// Loads, resolves, and lints one bounded project.
    pub fn load_and_lint(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
    ) -> Result<ProjectReport, ProjectLoadError> {
        Ok(self.load_and_lint_with_metrics(linter, selection)?.0)
    }

    pub fn load_and_lint_with_metrics(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
    ) -> Result<(ProjectReport, ProjectLoadMetrics), ProjectLoadError> {
        let mut metrics = ProjectLoadMetrics::default();
        let total_start = Instant::now();
        let report = self.load_project(linter, selection, &mut metrics)?;
        metrics.total = total_start.elapsed();
        Ok((report, metrics))
    }

    fn load_project(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<ProjectReport, ProjectLoadError> {
        let discovery_start = Instant::now();
        let paths = ProjectPaths::from_selection(&self.options, selection)?;
        metrics.discovery += discovery_start.elapsed();

        let mut build = ProjectBuild::new(linter, &self.options, paths.root, selection)?;
        build.add_initial_paths(paths.initial_paths);
        build.load_all(metrics)?;
        build.finish(metrics)
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
        let initial_paths =
            ProjectDiscovery::new(options).initial_paths(selection, &selection_path, &root)?;
        Ok(Self {
            root,
            initial_paths: initial_paths.into(),
        })
    }
}

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
struct ResolutionCache(BTreeMap<(String, ResolutionRequestKind, String), ResolutionResult>);
impl ResolutionCache {
    fn get(&self, key: &(String, ResolutionRequestKind, String)) -> Option<&ResolutionResult> {
        self.0.get(key)
    }

    fn insert(&mut self, key: (String, ResolutionRequestKind, String), result: ResolutionResult) {
        self.0.insert(key, result);
    }
}

#[derive(Debug, Default)]
struct LoadCounters {
    requests: usize,
    edges: usize,
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
struct ProjectBuild<'a> {
    session: glass_lint_core::ProjectSession<'a>,
    discovery: ProjectDiscovery<'a>,
    resolver: ProjectResolver,
    root: PathBuf,
    queue: PathWorkQueue,
    admitted: AdmissionSet,
    resolved: ResolutionCache,
    counters: LoadCounters,
}

impl<'a> ProjectBuild<'a> {
    fn new(
        linter: &'a Linter,
        options: &'a ProjectLoadOptions,
        root: PathBuf,
        selection: &ProjectSelection,
    ) -> Result<Self, ProjectLoadError> {
        Ok(Self {
            session: linter.begin_project(&root)?,
            discovery: ProjectDiscovery::new(options),
            resolver: ProjectResolver::new(&root, selection, options),
            root,
            queue: PathWorkQueue::default(),
            admitted: AdmissionSet::default(),
            resolved: ResolutionCache::default(),
            counters: LoadCounters::default(),
        })
    }

    fn add_initial_paths(&mut self, paths: VecDeque<PathBuf>) {
        self.queue.extend(paths);
    }

    fn load_all(&mut self, metrics: &mut ProjectLoadMetrics) -> Result<(), ProjectLoadError> {
        while let Some(path) = self.queue.pop_front() {
            self.load_path(&path, metrics)?;
        }
        Ok(())
    }

    fn load_path(
        &mut self,
        path: &Path,
        metrics: &mut ProjectLoadMetrics,
    ) -> Result<(), ProjectLoadError> {
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

        let parse_start = Instant::now();
        let requests = self.session.add_source(source)?;
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
            let cache_key = (
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

    fn enqueue_internal_target(
        &mut self,
        result: &ResolutionResult,
        metrics: &mut ProjectLoadMetrics,
    ) {
        if let ResolutionResult::Internal { path } = result {
            self.counters.record_edge();
            metrics.edges = self.counters.edges;
            let target = self.root.join(path);
            if target.exists()
                && !crate::discovery::excluded_path(
                    &self.root,
                    &target,
                    &self.discovery.options().excluded_directories,
                )
                && crate::discovery::supported_path(&target, &self.discovery.options().extensions)
            {
                self.queue.push(target);
            }
        }
    }

    fn finish(self, metrics: &mut ProjectLoadMetrics) -> Result<ProjectReport, ProjectLoadError> {
        let link_start = Instant::now();
        let (report, linking, matching) = self.session.finish_with_timings()?;
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
        ProjectSelection::Entry(_) | ProjectSelection::TsConfig(_) => {
            path.parent().unwrap_or(path).to_path_buf()
        }
    }
}
