//! Public project loading API and the bounded construction loop.

use std::{
    collections::{BTreeMap, VecDeque},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use glass_lint_core::{
    Linter,
    project::{
        AnalysisReport, ProjectRelativePath, ResolutionRequest, ResolverOutcome, SourceFile,
        SourceText,
    },
};

pub use crate::loader_metrics::{ProjectLoadMetrics, ProjectPhaseTimings};
use crate::{
    admission::{AdmissionSet, AdmittedSourcePath, SourceAdmission, absolute_path},
    budget::ProjectResourceBudget,
    discovery::{DiscoveryResult, ProjectDiscovery},
    error::ProjectLoadError,
    loader_metrics::LoadAccounting,
    loader_phases::{PathWorkQueue, ResolutionCache},
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
    /// Source text retained from the admitted files for presentation layers.
    /// The report itself remains source-free and serializable.
    pub sources: BTreeMap<ProjectRelativePath, SourceText>,
    /// Recoverable boundary error that caused the partial report. Fatal
    /// errors, including timeout, are returned through the outer `Result`.
    pub partial_reason: Option<ProjectLoadError>,
    /// Phase timings and deterministic counters for this load.
    pub metrics: ProjectLoadMetrics,
}

impl ProjectLoadOutcome {
    fn complete(
        report: AnalysisReport,
        sources: BTreeMap<ProjectRelativePath, SourceText>,
    ) -> Self {
        Self {
            report,
            sources,
            partial_reason: None,
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
            partial_reason: Some(reason),
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
        let mut metrics = LoadAccounting::default();
        let total_start = Instant::now();
        let mut outcome = self.load_project_with_outcome(linter, selection, &mut metrics)?;
        metrics.record_total(total_start.elapsed());
        outcome.metrics = metrics.snapshot();
        Ok(outcome)
    }

    fn load_project_with_outcome(
        &self,
        linter: &Linter,
        selection: &ProjectSelection,
        metrics: &mut LoadAccounting,
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
            paths.admission,
            paths.diagnostics,
            selection,
            deadline,
        )?;
        build.add_initial_paths(paths.initial_paths);
        let (expansion_result, closed) = build.close_frontier(metrics);
        match expansion_result {
            Ok(()) => {
                let (report, sources) = closed.finish(FinishMode::Complete, metrics)?;
                Ok(ProjectLoadOutcome::complete(report, sources))
            }
            Err(ProjectLoadError::Timeout) => Err(ProjectLoadError::Timeout),
            Err(error) => {
                let (report, sources) = closed.finish(FinishMode::Partial, metrics)?;
                Ok(ProjectLoadOutcome::partial(report, sources, error))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LoadDeadline(Instant);

impl LoadDeadline {
    fn after_millis(timeout_ms: u64) -> Self {
        Self(Instant::now() + Duration::from_millis(timeout_ms))
    }

    fn instant(self) -> Instant {
        self.0
    }

    fn check(self) -> Result<(), ProjectLoadError> {
        (Instant::now() <= self.0)
            .then_some(())
            .ok_or(ProjectLoadError::Timeout)
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
        budget: &mut ProjectResourceBudget,
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
            budget,
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

/// Result of admitting and reading one work queue wave. A source-byte budget
/// failure is deferred until the successfully read sources have been
/// analyzed, preserving deterministic partial output.
struct ReadWaveOutcome {
    sources: Vec<SourceFile>,
    deferred_error: Option<ProjectLoadError>,
}

/// Resolver output that the coordinator can apply to the frontier in request
/// order after resolution policy and cache state have been handled.
struct RequestResolutionOutcome {
    internal_targets: Vec<ProjectRelativePath>,
    elapsed: Duration,
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
    /// Cache mapping already-admitted internal target paths to their
    /// AdmittedSourcePath, avoiding redundant exists/classify calls when
    /// multiple importers reference the same target.
    admitted_target_cache: BTreeMap<ProjectRelativePath, AdmittedSourcePath>,
    /// Source text retained for presentation after the core report is built.
    sources: BTreeMap<ProjectRelativePath, SourceText>,
    deadline: LoadDeadline,
}

impl<'a> ProjectLoadState<'a> {
    fn new(
        linter: &'a Linter,
        admission: SourceAdmission<'a>,
        diagnostics: Vec<crate::tsconfig::TsconfigDiagnostic>,
        selection: &ProjectSelection,
        deadline: LoadDeadline,
    ) -> Result<Self, ProjectLoadError> {
        let session = linter.begin_project()?;
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
            admitted_target_cache: BTreeMap::new(),
            sources: BTreeMap::new(),
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
        metrics: &mut LoadAccounting,
    ) -> (Result<(), ProjectLoadError>, ClosedFrontier<'a>) {
        let workers = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

        let result = loop {
            if let Err(e) = self.deadline.check() {
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
            sources: self.sources,
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
        metrics: &mut LoadAccounting,
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
            metrics.record_files(self.admitted.len());

            metrics.admit_requests(requests.len(), self.admission.options().max_requests())?;

            let resolution = self.resolve_requests(requests)?;
            metrics.record_resolution(resolution.elapsed);
            self.apply_request_resolution(resolution, metrics)?;
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
        wave: &[AdmittedSourcePath],
        metrics: &mut LoadAccounting,
    ) -> Result<ReadWaveOutcome, ProjectLoadError> {
        let source_limit = self.admission.options().max_project_source_bytes();
        let mut sources = Vec::with_capacity(wave.len());
        let mut deferred_error = None;
        for admitted in wave {
            if !self.admitted.admit(admitted)? {
                continue;
            }

            // Check the cumulative byte budget against the on-disk size
            // before reading, so a file at the boundary is rejected without
            // wasting I/O.
            let md =
                std::fs::metadata(admitted.as_ref()).map_err(|source| ProjectLoadError::Io {
                    path: admitted.as_ref().to_path_buf(),
                    source,
                })?;
            if metrics.source_bytes().saturating_add(md.len()) > source_limit {
                deferred_error = Some(ProjectLoadError::ProjectSourceTooLarge {
                    bytes: metrics.source_bytes().saturating_add(md.len()),
                    limit: source_limit,
                });
                break;
            }

            let source = self.admission.load_admitted_source_file(admitted)?;
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
    ) -> Result<Vec<ResolutionRequest>, ProjectLoadError> {
        self.session
            .analyze_sources(sources, workers)
            .map_err(ProjectLoadError::from)
    }

    fn resolve_requests(
        &mut self,
        requests: Vec<ResolutionRequest>,
    ) -> Result<RequestResolutionOutcome, ProjectLoadError> {
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
        Ok(RequestResolutionOutcome {
            internal_targets,
            elapsed,
        })
    }

    fn apply_request_resolution(
        &mut self,
        resolution: RequestResolutionOutcome,
        metrics: &mut LoadAccounting,
    ) -> Result<(), ProjectLoadError> {
        for path in resolution.internal_targets {
            self.enqueue_internal_target(path, metrics)?;
        }
        Ok(())
    }

    fn enqueue_internal_target(
        &mut self,
        path: glass_lint_core::project::ProjectRelativePath,
        metrics: &mut LoadAccounting,
    ) -> Result<(), ProjectLoadError> {
        metrics.record_edge();
        if let Some(admitted) = self.admitted_target_cache.get(&path) {
            self.queue.push(admitted.clone());
            return Ok(());
        }
        let target = self.admission.canonical_root().join(&path);
        if target.exists()
            && let crate::admission::PathAdmission::Admitted(admitted) =
                self.admission.classify(&target)?
        {
            self.admitted_target_cache.insert(path, admitted.clone());
            self.queue.push(admitted);
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
    sources: BTreeMap<ProjectRelativePath, SourceText>,
    deadline: LoadDeadline,
}

#[derive(Clone, Copy)]
enum FinishMode {
    Complete,
    Partial,
}

impl ClosedFrontier<'_> {
    fn finish(
        self,
        mode: FinishMode,
        metrics: &mut LoadAccounting,
    ) -> Result<(AnalysisReport, BTreeMap<ProjectRelativePath, SourceText>), ProjectLoadError> {
        if matches!(mode, FinishMode::Complete) {
            self.deadline.check()?;
        }
        self.finish_inner(metrics)
    }

    fn finish_inner(
        self,
        metrics: &mut LoadAccounting,
    ) -> Result<(AnalysisReport, BTreeMap<ProjectRelativePath, SourceText>), ProjectLoadError> {
        let sources = self.sources;
        let local = self.session.finish_local();
        let resolved = local.resolve(self.resolved.into_iter())?;
        let result = resolved.finish_with_timings()?;
        metrics.record_linking(result.linking);
        metrics.record_matching(result.matching);
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
        let report = result.report.with_project_diagnostics(&code, messages);
        Ok((report, sources))
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
