//! Deterministic project admission, local analysis, and staging.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeMap, num::NonZeroUsize, sync::Arc};

struct LocalJob {
    path: ProjectRelativePath,
    source: SourceFile,
    key: ArtifactCacheKey,
}

struct LocalJobResult {
    path: ProjectRelativePath,
    source: SourceFile,
    key: ArtifactCacheKey,
    result: Result<Arc<crate::analysis::SemanticArtifact>, ParseDiagnostic>,
}

trait LocalJobExecutor {
    fn execute(
        &self,
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) -> Result<(), LocalExecutionError>;
}

#[derive(Clone, Copy, Debug)]
enum ExecutionEvent {
    Submitted,
    Started,
    Finished,
    Merged,
    ParseAttempted,
    LowerAttempted,
    CacheHit,
    CacheMiss,
    CacheInserted,
    CacheEvicted,
}

trait ExecutionObserver: Send + Sync {
    fn observe(&self, event: ExecutionEvent);
}

struct NoopExecutionObserver;
impl ExecutionObserver for NoopExecutionObserver {
    fn observe(&self, _event: ExecutionEvent) {}
}

#[cfg(test)]
pub(super) struct CountingExecutionObserver {
    active: AtomicUsize,
    peak_active: AtomicUsize,
    outstanding: AtomicUsize,
    peak_outstanding: AtomicUsize,
    parse_attempts: AtomicUsize,
    lower_attempts: AtomicUsize,
    cache_hits: AtomicUsize,
    cache_misses: AtomicUsize,
    cache_inserts: AtomicUsize,
    cache_evictions: AtomicUsize,
}

#[cfg(test)]
impl CountingExecutionObserver {
    pub(super) fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            peak_active: AtomicUsize::new(0),
            outstanding: AtomicUsize::new(0),
            peak_outstanding: AtomicUsize::new(0),
            parse_attempts: AtomicUsize::new(0),
            lower_attempts: AtomicUsize::new(0),
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            cache_inserts: AtomicUsize::new(0),
            cache_evictions: AtomicUsize::new(0),
        }
    }

    pub(super) fn peaks(&self) -> (usize, usize) {
        (
            self.peak_active.load(Ordering::SeqCst),
            self.peak_outstanding.load(Ordering::SeqCst),
        )
    }

    pub(super) fn invocations(&self) -> InvocationCounts {
        InvocationCounts {
            parses: self.parse_attempts.load(Ordering::SeqCst),
            lowers: self.lower_attempts.load(Ordering::SeqCst),
            hits: self.cache_hits.load(Ordering::SeqCst),
            misses: self.cache_misses.load(Ordering::SeqCst),
            inserts: self.cache_inserts.load(Ordering::SeqCst),
            evictions: self.cache_evictions.load(Ordering::SeqCst),
        }
    }

    fn peak(slot: &AtomicUsize, value: usize) {
        let _ = slot.fetch_max(value, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl ExecutionObserver for CountingExecutionObserver {
    fn observe(&self, event: ExecutionEvent) {
        match event {
            ExecutionEvent::Submitted => {
                let value = self.outstanding.fetch_add(1, Ordering::SeqCst) + 1;
                Self::peak(&self.peak_outstanding, value);
            }
            ExecutionEvent::Started => {
                let value = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                Self::peak(&self.peak_active, value);
            }
            ExecutionEvent::Finished => {
                self.active.fetch_sub(1, Ordering::SeqCst);
            }
            ExecutionEvent::Merged => {
                self.outstanding.fetch_sub(1, Ordering::SeqCst);
            }
            ExecutionEvent::ParseAttempted => {
                self.parse_attempts.fetch_add(1, Ordering::SeqCst);
            }
            ExecutionEvent::LowerAttempted => {
                self.lower_attempts.fetch_add(1, Ordering::SeqCst);
            }
            ExecutionEvent::CacheHit => {
                self.cache_hits.fetch_add(1, Ordering::SeqCst);
            }
            ExecutionEvent::CacheMiss => {
                self.cache_misses.fetch_add(1, Ordering::SeqCst);
            }
            ExecutionEvent::CacheInserted => {
                self.cache_inserts.fetch_add(1, Ordering::SeqCst);
            }
            ExecutionEvent::CacheEvicted => {
                self.cache_evictions.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct InvocationCounts {
    pub(super) parses: usize,
    pub(super) lowers: usize,
    pub(super) hits: usize,
    pub(super) misses: usize,
    pub(super) inserts: usize,
    pub(super) evictions: usize,
}

struct ThreadLocalJobExecutor;

impl LocalJobExecutor for ThreadLocalJobExecutor {
    fn execute(
        &self,
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) -> Result<(), LocalExecutionError> {
        let bound = outstanding_job_bound(worker_limit);
        let (job_tx, job_rx) = std::sync::mpsc::sync_channel::<LocalJob>(bound);
        let (result_tx, result_rx) = std::sync::mpsc::sync_channel(bound);
        let queue = std::sync::Mutex::new(job_rx);
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..worker_limit.get() {
                let queue_ref = &queue;
                let result_tx = result_tx.clone();
                handles.push(scope.spawn(move || {
                    loop {
                        let job = queue_ref
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .recv();
                        let Ok(job) = job else {
                            break;
                        };
                        observer.observe(ExecutionEvent::Started);
                        observer.observe(ExecutionEvent::ParseAttempted);
                        observer.observe(ExecutionEvent::LowerAttempted);
                        let result = crate::analysis::lower_source(linter, &job.source)
                            .map(|ls| ls.semantic);
                        observer.observe(ExecutionEvent::Finished);
                        result_tx
                            .send(LocalJobResult {
                                path: job.path,
                                source: job.source,
                                key: job.key,
                                result,
                            })
                            .map_err(|_| LocalExecutionError::WorkerPanic)?;
                    }
                    Ok::<_, LocalExecutionError>(())
                }));
            }

            let mut outstanding = 0usize;
            for job in jobs {
                observer.observe(ExecutionEvent::Submitted);
                job_tx
                    .send(job)
                    .map_err(|_| LocalExecutionError::WorkerPanic)?;
                outstanding += 1;
                if outstanding == bound {
                    release(
                        result_rx
                            .recv()
                            .map_err(|_| LocalExecutionError::WorkerPanic)?,
                    );
                    outstanding -= 1;
                }
            }
            drop(job_tx);
            while outstanding != 0 {
                release(
                    result_rx
                        .recv()
                        .map_err(|_| LocalExecutionError::WorkerPanic)?,
                );
                outstanding -= 1;
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|_| LocalExecutionError::WorkerPanic)??;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum ControlledReleaseOrder {
    Forward,
    Reverse,
    Interleaved,
}

#[cfg(test)]
pub(super) struct ControlledLocalJobExecutor(ControlledReleaseOrder);

#[cfg(test)]
impl LocalJobExecutor for ControlledLocalJobExecutor {
    fn execute(
        &self,
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        _worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) -> Result<(), LocalExecutionError> {
        let all: Vec<_> = jobs.collect();
        let indexes: Vec<usize> = match self.0 {
            ControlledReleaseOrder::Forward => (0..all.len()).collect(),
            ControlledReleaseOrder::Reverse => (0..all.len()).rev().collect(),
            ControlledReleaseOrder::Interleaved => (0..all.len())
                .step_by(2)
                .chain((1..all.len()).step_by(2))
                .collect(),
        };
        let mut jobs: Vec<_> = all.into_iter().map(Some).collect();
        for index in indexes {
            let job = jobs[index].take().expect("release index is unique");
            observer.observe(ExecutionEvent::Submitted);
            observer.observe(ExecutionEvent::Started);
            observer.observe(ExecutionEvent::ParseAttempted);
            observer.observe(ExecutionEvent::LowerAttempted);
            let result = crate::analysis::lower_source(linter, &job.source).map(|ls| ls.semantic);
            observer.observe(ExecutionEvent::Finished);
            release(LocalJobResult {
                path: job.path,
                source: job.source,
                key: job.key,
                result,
            });
        }
        Ok(())
    }
}

fn normalize_worker_limit(requested: usize) -> NonZeroUsize {
    NonZeroUsize::new(requested).unwrap_or(NonZeroUsize::MIN)
}

#[derive(Default)]
struct AnalysisArtifacts {
    authored_requests: BTreeMap<ResolutionRequestKey, ResolutionRequest>,
    analyzed: BTreeMap<ProjectRelativePath, LocalArtifact>,
    parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
}

/// Authored module requests produced by one completed local source analysis.
/// Source and artifact storage remains owned by the collection phase.
pub struct SourceAnalysis {
    requests: Vec<ResolutionRequest>,
}

impl SourceAnalysis {
    pub fn requests(self) -> Vec<ResolutionRequest> {
        self.requests
    }

    pub fn requests_ref(&self) -> &[ResolutionRequest] {
        &self.requests
    }
}

impl AnalysisArtifacts {
    fn record_parse_failure(&mut self, path: ProjectRelativePath, error: ParseDiagnostic) {
        self.analyzed.remove(&path);
        self.parse_diagnostics.insert(path, error);
    }

    fn record_lowered(
        &mut self,
        path: &ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        let local = LocalArtifact::new(lowered.source.clone(), lowered.semantic);
        let requests = local
            .interface()
            .authored_requests(path, &local.source_context().lines);
        for request in &requests {
            self.authored_requests
                .insert(request.key.clone(), request.clone());
        }
        self.analyzed.insert(path.clone(), local);
        requests
    }
}

pub(super) const fn outstanding_job_bound(worker_limit: NonZeroUsize) -> usize {
    worker_limit.get().saturating_mul(2)
}

/// Outcome of looking up a source in the artifact cache.
enum CacheLookup {
    Hit(LoweredSource),
    Miss(ArtifactCacheKey),
}

fn cached_lowered_source(source: &SourceFile, cached: &SharedSemanticArtifact) -> LoweredSource {
    LoweredSource {
        source: LocatedSourceContext::new(source),
        semantic: Arc::clone(&cached.semantic),
    }
}

fn insert_and_notify(
    cache: &ArtifactCacheHandle,
    key: ArtifactCacheKey,
    semantic: &Arc<SemanticArtifact>,
    observer: &dyn ExecutionObserver,
) {
    let evicted = cache.insert(
        key,
        SharedSemanticArtifact {
            semantic: Arc::clone(semantic),
        },
    );
    observer.observe(ExecutionEvent::CacheInserted);
    if evicted {
        observer.observe(ExecutionEvent::CacheEvicted);
    }
}

use crate::{
    Linter, LocalExecutionError, ParseDiagnostic, ProjectRelativePath,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, LoweredSource,
        QualifiedRequestId, SemanticArtifact, SharedSemanticArtifact,
    },
    project::{
        AnalysisReport, ProjectInputError, ResolutionRequest, ResolutionRequestKey,
        ResolverOutcome, SourceFile,
        input::{
            ValidatedProjectInput, normalize_relative, normalize_resolution_key, normalize_result,
            normalize_root,
        },
        tables::{ResolutionTable, SourceTable},
    },
};

pub struct ProjectCollection<'a> {
    pub(super) linter: &'a Linter,
    pub(super) root: std::path::PathBuf,
    pub(super) sources: SourceTable,
    artifacts: AnalysisArtifacts,
    pub(super) artifact_cache: ArtifactCacheHandle,
    #[cfg(test)]
    fingerprint_engine_version: &'static str,
    #[cfg(test)]
    fingerprint_normalization: Option<&'static str>,
}

/// Project state after every admitted source has completed local analysis.
/// The consuming transition prevents adding sources after this point.
pub struct LocallyAnalyzedProject<'a> {
    linter: &'a Linter,
    root: std::path::PathBuf,
    sources: SourceTable,
    artifacts: AnalysisArtifacts,
}

/// Project state after the authored resolution table has been validated.
/// Linking and matching are available only from this phase.
pub struct ResolvedProject<'a> {
    linter: &'a Linter,
    input: ValidatedProjectInput,
    artifacts: AnalysisArtifacts,
}

impl<'a> ProjectCollection<'a> {
    #[cfg(test)]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        if self.fingerprint_normalization.is_none()
            && self.fingerprint_engine_version == env!("CARGO_PKG_VERSION")
        {
            return ArtifactCacheKey::new(
                source,
                self.linter.analysis_environment(),
                self.linter.analysis_limits(),
            );
        }
        self.fingerprint_normalization.map_or_else(
            || {
                ArtifactCacheKey::for_engine_version(
                    source,
                    self.linter.analysis_environment(),
                    self.linter.analysis_limits(),
                    self.fingerprint_engine_version,
                )
            },
            |normalization| {
                ArtifactCacheKey::for_test_inputs(
                    source,
                    self.linter.analysis_environment(),
                    self.linter.analysis_limits(),
                    normalization,
                    self.fingerprint_engine_version,
                )
            },
        )
    }

    #[cfg(not(test))]
    fn artifact_fingerprint(&self, source: &SourceFile) -> ArtifactCacheKey {
        ArtifactCacheKey::new(
            source,
            self.linter.analysis_environment(),
            self.linter.analysis_limits(),
        )
    }

    /// Check the artifact cache for a source, returning either a cached
    /// lowered source or the key needed to lower and cache it.
    fn check_cache(&self, source: &SourceFile, observer: &dyn ExecutionObserver) -> CacheLookup {
        let key = self.artifact_fingerprint(source);
        self.artifact_cache.get(&key).map_or_else(
            || {
                observer.observe(ExecutionEvent::CacheMiss);
                CacheLookup::Miss(key)
            },
            |cached| {
                observer.observe(ExecutionEvent::CacheHit);
                CacheLookup::Hit(cached_lowered_source(source, &cached))
            },
        )
    }

    /// Start an empty parse-once project session under a canonical root.
    pub fn new(
        linter: &'a Linter,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ProjectInputError> {
        Ok(Self {
            linter,
            root: normalize_root(&root.into())?,
            sources: SourceTable::default(),
            artifacts: AnalysisArtifacts::default(),
            artifact_cache: linter.artifact_cache_handle(),
            #[cfg(test)]
            fingerprint_engine_version: env!("CARGO_PKG_VERSION"),
            #[cfg(test)]
            fingerprint_normalization: None,
        })
    }

    fn admit_normalized_source(&mut self, mut source: SourceFile) -> Result<(), ProjectInputError> {
        source.path = normalize_relative(&source.path)?;
        self.sources.insert(source)
    }

    pub(crate) fn admit_validated_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.sources.insert(source)
    }

    /// Analyze one owned source and return its authored requests.
    pub fn analyze_source(
        &mut self,
        source: SourceFile,
    ) -> Result<SourceAnalysis, ProjectInputError> {
        let path = source.path.clone();
        self.admit_normalized_source(source)?;
        Ok(SourceAnalysis {
            requests: self.analyze_source_at_path(&path)?,
        })
    }

    #[cfg(test)]
    fn analyze_source_with_observer(
        &mut self,
        path: impl AsRef<str>,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let path = normalize_relative(path.as_ref())?;
        self.analyze_source_at_path_with_observer(&path, observer)
    }

    fn analyze_source_at_path_with_observer(
        &mut self,
        path: &ProjectRelativePath,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let source = self
            .sources
            .get(path.as_str())
            .ok_or_else(|| ProjectInputError::InvalidPath(path.to_string()))?;
        let lowered = match self.check_cache(source, observer) {
            CacheLookup::Hit(lowered) => lowered,
            CacheLookup::Miss(key) => {
                observer.observe(ExecutionEvent::ParseAttempted);
                observer.observe(ExecutionEvent::LowerAttempted);
                let lowered = match crate::analysis::lower_source(self.linter, source) {
                    Ok(lowered) => lowered,
                    Err(error) => {
                        self.artifacts.record_parse_failure(path.clone(), error);
                        return Ok(Vec::new());
                    }
                };
                insert_and_notify(&self.artifact_cache, key, &lowered.semantic, observer);
                lowered
            }
        };
        Ok(self.record_lowered(path, lowered))
    }

    pub(crate) fn analyze_source_at_path(
        &mut self,
        path: &ProjectRelativePath,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_at_path_with_observer(path, &NoopExecutionObserver)
    }

    #[cfg(test)]
    pub(super) fn analyze_source_counted(
        &mut self,
        path: impl AsRef<str>,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_with_observer(path, observer)
    }

    #[cfg(test)]
    pub(super) fn admit_test_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.admit_normalized_source(source)
    }

    fn record_lowered(
        &mut self,
        path: &ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        self.artifacts.record_lowered(path, lowered)
    }

    /// Analyze all admitted sources using a bounded worker count. Canonical
    /// maps and final request sorting make results independent of worker count
    /// and task completion order.
    fn analyze_pending_sources(
        &mut self,
        worker_count: usize,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let observer = NoopExecutionObserver;
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, &observer)
    }

    /// Admit and analyze owned sources with bounded local execution.
    pub fn analyze_sources(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        workers: NonZeroUsize,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
        self.analyze_pending_sources(workers.get())
    }

    fn analyze_pending_sources_with<E: LocalJobExecutor>(
        &mut self,
        worker_count: usize,
        executor: &E,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let worker_count = normalize_worker_limit(worker_count);
        let pending: Vec<_> = self
            .sources
            .iter()
            .filter(|(path, _)| {
                !self.artifacts.analyzed.contains_key(*path)
                    && !self.artifacts.parse_diagnostics.contains_key(*path)
            })
            .map(|(path, _)| path.to_owned())
            .collect();
        let mut requests = Vec::new();
        let mut uncached = Vec::new();
        for pending_path in pending {
            let path = ProjectRelativePath::new(&pending_path)?;
            let Some(source) = self.sources.get(&pending_path) else {
                continue;
            };
            match self.check_cache(source, observer) {
                CacheLookup::Hit(lowered) => {
                    requests.extend(self.record_lowered(&path, lowered));
                }
                CacheLookup::Miss(key) => {
                    uncached.push(LocalJob {
                        path,
                        source: source.clone(),
                        key,
                    });
                }
            }
        }

        let artifact_cache = self.artifact_cache.clone();
        let artifacts = &mut self.artifacts;
        let mut release = |result: LocalJobResult| {
            match result.result {
                Ok(artifact) => {
                    insert_and_notify(&artifact_cache, result.key, &artifact, observer);
                    requests.extend(artifacts.record_lowered(
                        &result.path,
                        LoweredSource {
                            source: LocatedSourceContext::new(&result.source),
                            semantic: artifact,
                        },
                    ));
                }
                Err(error) => {
                    artifacts.record_parse_failure(result.path, error);
                }
            }
            observer.observe(ExecutionEvent::Merged);
        };
        executor
            .execute(
                Box::new(uncached.into_iter()),
                worker_count,
                self.linter,
                observer,
                &mut release,
            )
            .map_err(ProjectInputError::LocalExecution)?;
        requests.sort_by(|left, right| {
            (
                left.key.importer.as_str(),
                left.key.kind,
                &left.key.range,
                left.request.as_str(),
            )
                .cmp(&(
                    right.key.importer.as_str(),
                    right.key.kind,
                    &right.key.range,
                    right.request.as_str(),
                ))
        });
        Ok(requests)
    }

    #[cfg(test)]
    pub(super) fn analyze_sources_controlled(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        worker_count: usize,
        order: ControlledReleaseOrder,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
        let observer = NoopExecutionObserver;
        self.analyze_pending_sources_with(
            worker_count,
            &ControlledLocalJobExecutor(order),
            &observer,
        )
    }

    #[cfg(test)]
    pub(super) fn analyze_sources_counted(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        worker_count: usize,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        for source in sources {
            self.admit_normalized_source(source)?;
        }
        self.analyze_pending_sources_with(worker_count, &ThreadLocalJobExecutor, observer)
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_engine_version(&mut self, version: &'static str) {
        self.fingerprint_engine_version = version;
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_normalization(&mut self, normalization: &'static str) {
        self.fingerprint_normalization = Some(normalization);
    }

    /// Consume the collection after local analysis and freeze its authored
    /// request set for the resolution phase.
    pub fn finish_local(self) -> LocallyAnalyzedProject<'a> {
        LocallyAnalyzedProject {
            linter: self.linter,
            root: self.root,
            sources: self.sources,
            artifacts: self.artifacts,
        }
    }
}

impl<'a> LocallyAnalyzedProject<'a> {
    /// Validate resolver outcomes against the frozen authored request table
    /// and build the qualified-request-identity table that linking consumes.
    pub fn resolve(
        self,
        outcomes: impl IntoIterator<Item = (ResolutionRequestKey, ResolverOutcome)>,
    ) -> Result<ResolvedProject<'a>, ProjectInputError> {
        let mut resolutions = ResolutionTable::default();
        for (mut key, mut result) in outcomes {
            normalize_resolution_key(&mut key)?;
            if !self.artifacts.authored_requests.contains_key(&key) {
                return Err(ProjectInputError::UnknownRequest(key));
            }
            normalize_result(&mut result)?;
            resolutions.insert(key, result)?;
        }
        let input = ValidatedProjectInput::from_maps(
            self.root,
            self.sources.into_map(),
            resolutions.into_map(),
        );
        let request_ids: BTreeMap<ResolutionRequestKey, QualifiedRequestId> = self
            .artifacts
            .analyzed
            .iter()
            .filter_map(|(path, local)| {
                let module_id = input.module_id(path)?;
                Some((path, local, module_id))
            })
            .flat_map(|(path, local, module_id)| {
                let interface = local.interface();
                let lines = &local.source_context().lines;
                let requests = interface.requests_with_ids(path, lines);
                requests
                    .into_iter()
                    .map(move |(req_id, authored)| {
                        (
                            authored.key,
                            QualifiedRequestId {
                                module: module_id,
                                request: req_id,
                            },
                        )
                    })
            })
            .collect();
        Ok(ResolvedProject {
            linter: self.linter,
            input: input.with_request_ids(request_ids),
            artifacts: self.artifacts,
        })
    }
}

impl ResolvedProject<'_> {
    /// Link, match, and assemble the report. This consuming method cannot be
    /// called twice because the resolved project is moved into the pipeline.
    pub fn finish(self) -> Result<AnalysisReport, ProjectInputError> {
        self.finish_with_timings().map(|result| result.report)
    }

    pub fn finish_with_timings(self) -> Result<crate::lint::ProjectAnalysis, ProjectInputError> {
        self.linter.finish_analyzed_project(
            self.input,
            self.artifacts.analyzed,
            self.artifacts.parse_diagnostics,
        )
    }
}
