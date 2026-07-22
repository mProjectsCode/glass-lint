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
        jobs: Vec<LocalJob>,
        worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    );
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
        jobs: Vec<LocalJob>,
        worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) {
        let bound = outstanding_job_bound(worker_limit);
        let mut remaining = jobs;
        while !remaining.is_empty() {
            let take = bound.min(remaining.len());
            let batch: Vec<LocalJob> = remaining.drain(..take).collect();
            let batch_size = batch.len();
            for _ in 0..batch_size {
                observer.observe(ExecutionEvent::Submitted);
            }
            let chunk_size = batch_size.max(1).div_ceil(worker_limit.get());
            // Split the owned batch into owned per-worker chunks so each
            // worker moves job fields into LocalJobResult without cloning.
            let mut worker_chunks: Vec<Vec<LocalJob>> = Vec::new();
            let mut remaining_batch = batch;
            while !remaining_batch.is_empty() {
                let take = chunk_size.min(remaining_batch.len());
                worker_chunks.push(remaining_batch.drain(..take).collect());
            }
            let batch_results = std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for worker_jobs in worker_chunks {
                    handles.push(scope.spawn(move || {
                        worker_jobs
                            .into_iter()
                            .map(|job| {
                                observer.observe(ExecutionEvent::Started);
                                observer.observe(ExecutionEvent::ParseAttempted);
                                observer.observe(ExecutionEvent::LowerAttempted);
                                let result = crate::analysis::lower_source(linter, &job.source)
                                    .map(|ls| ls.semantic);
                                observer.observe(ExecutionEvent::Finished);
                                LocalJobResult {
                                    path: job.path,
                                    source: job.source,
                                    key: job.key,
                                    result,
                                }
                            })
                            .collect::<Vec<_>>()
                    }));
                }
                handles
                    .into_iter()
                    .flat_map(|handle| handle.join().expect("analysis worker panicked"))
                    .collect::<Vec<_>>()
            });
            for result in batch_results {
                release(result);
            }
        }
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
        jobs: Vec<LocalJob>,
        _worker_limit: NonZeroUsize,
        linter: &Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) {
        let mut jobs = jobs.into_iter().map(Some).collect::<Vec<_>>();
        let indexes: Vec<usize> = match self.0 {
            ControlledReleaseOrder::Forward => (0..jobs.len()).collect(),
            ControlledReleaseOrder::Reverse => (0..jobs.len()).rev().collect(),
            ControlledReleaseOrder::Interleaved => (0..jobs.len())
                .step_by(2)
                .chain((1..jobs.len()).step_by(2))
                .collect(),
        };
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
    Linter, ParseDiagnostic, ProjectRelativePath,
    analysis::{
        ArtifactCacheHandle, ArtifactCacheKey, LocalArtifact, LocatedSourceContext, LoweredSource,
        SemanticArtifact, SharedSemanticArtifact,
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

pub struct AnalysisSession<'a> {
    pub(super) linter: &'a Linter,
    pub(super) root: std::path::PathBuf,
    pub(super) sources: SourceTable,
    pub(super) resolutions: ResolutionTable,
    artifacts: AnalysisArtifacts,
    pub(super) artifact_cache: ArtifactCacheHandle,
    #[cfg(test)]
    fingerprint_engine_version: &'static str,
    #[cfg(test)]
    fingerprint_normalization: Option<&'static str>,
}

impl<'a> AnalysisSession<'a> {
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
            resolutions: ResolutionTable::default(),
            artifacts: AnalysisArtifacts::default(),
            artifact_cache: linter.artifact_cache_handle(),
            #[cfg(test)]
            fingerprint_engine_version: env!("CARGO_PKG_VERSION"),
            #[cfg(test)]
            fingerprint_normalization: None,
        })
    }

    /// Normalize and admit one source file without starting semantic analysis.
    pub fn admit_source(&mut self, mut source: SourceFile) -> Result<(), ProjectInputError> {
        source.path = normalize_relative(&source.path)?;
        self.sources.insert(source)
    }

    pub(crate) fn admit_validated_source(
        &mut self,
        source: SourceFile,
    ) -> Result<(), ProjectInputError> {
        self.sources.insert(source)
    }

    /// Analyze one previously admitted source and return its authored requests.
    pub fn analyze_source(
        &mut self,
        path: impl AsRef<str>,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_with_observer(path, &NoopExecutionObserver)
    }

    fn analyze_source_with_observer(
        &mut self,
        path: impl AsRef<str>,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let path = normalize_relative(path.as_ref())?;
        self.analyze_admitted_source_with_observer(&path, observer)
    }

    fn analyze_admitted_source_with_observer(
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

    pub(crate) fn analyze_admitted_source(
        &mut self,
        path: &ProjectRelativePath,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_admitted_source_with_observer(path, &NoopExecutionObserver)
    }

    #[cfg(test)]
    pub(super) fn analyze_source_counted(
        &mut self,
        path: impl AsRef<str>,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_source_with_observer(path, observer)
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
    pub fn analyze_admitted_sources(
        &mut self,
        worker_count: usize,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let observer = NoopExecutionObserver;
        self.analyze_admitted_sources_with(worker_count, &ThreadLocalJobExecutor, &observer)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn analyze_admitted_sources_with<E: LocalJobExecutor>(
        &mut self,
        worker_count: usize,
        executor: &E,
        observer: &dyn ExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let worker_count = normalize_worker_limit(worker_count);
        let pending = self
            .sources
            .iter()
            .filter(|(path, _)| {
                !self.artifacts.analyzed.contains_key(*path)
                    && !self.artifacts.parse_diagnostics.contains_key(*path)
            })
            .map(|(path, source)| (path.to_owned(), source.clone()))
            .collect::<Vec<_>>();
        let mut requests = Vec::new();
        let mut uncached = Vec::new();
        for (path, source) in pending {
            let path = ProjectRelativePath::new(path)?;
            match self.check_cache(&source, observer) {
                CacheLookup::Hit(lowered) => {
                    requests.extend(self.record_lowered(&path, lowered));
                }
                CacheLookup::Miss(key) => {
                    uncached.push(LocalJob { path, source, key });
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
        executor.execute(uncached, worker_count, self.linter, observer, &mut release);
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
    pub(super) fn analyze_admitted_sources_controlled(
        &mut self,
        worker_count: usize,
        order: ControlledReleaseOrder,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let observer = NoopExecutionObserver;
        self.analyze_admitted_sources_with(
            worker_count,
            &ControlledLocalJobExecutor(order),
            &observer,
        )
    }

    #[cfg(test)]
    pub(super) fn analyze_admitted_sources_counted(
        &mut self,
        worker_count: usize,
        observer: &CountingExecutionObserver,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        self.analyze_admitted_sources_with(worker_count, &ThreadLocalJobExecutor, observer)
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_engine_version(&mut self, version: &'static str) {
        self.fingerprint_engine_version = version;
    }

    #[cfg(test)]
    pub(super) fn set_fingerprint_normalization(&mut self, normalization: &'static str) {
        self.fingerprint_normalization = Some(normalization);
    }

    /// Normalize, admit, parse, and locally analyze one source file.
    pub fn add_source(
        &mut self,
        source: SourceFile,
    ) -> Result<Vec<ResolutionRequest>, ProjectInputError> {
        let path = normalize_relative(&source.path)?.to_string();
        self.admit_source(source)?;
        let path = ProjectRelativePath::new(path)?;
        self.analyze_admitted_source(&path)
    }

    /// Record a resolver answer only for an authored module request.
    pub fn record_resolution(
        &mut self,
        mut key: ResolutionRequestKey,
        mut result: ResolverOutcome,
    ) -> Result<(), ProjectInputError> {
        normalize_resolution_key(&mut key)?;
        if !self.artifacts.authored_requests.contains_key(&key) {
            return Err(ProjectInputError::UnknownRequest(key));
        }
        normalize_result(&mut result)?;
        self.resolutions.insert(key, result)
    }

    pub(crate) fn record_validated_resolution(
        &mut self,
        key: ResolutionRequestKey,
        result: ResolverOutcome,
    ) -> Result<(), ProjectInputError> {
        self.resolutions.insert(key, result)
    }

    /// Link the staged project and return its report.
    pub fn finish(self) -> Result<AnalysisReport, ProjectInputError> {
        self.finish_with_timings().map(|(report, _, _)| report)
    }

    /// Link the staged project and return report plus phase timings.
    pub fn finish_with_timings(
        self,
    ) -> Result<(AnalysisReport, std::time::Duration, std::time::Duration), ProjectInputError> {
        let input = ValidatedProjectInput::from_maps(
            self.root,
            self.sources.into_map(),
            self.resolutions.into_map(),
        );
        self.linter.finish_analyzed_project(
            input,
            self.artifacts.analyzed,
            self.artifacts.parse_diagnostics,
        )
    }
}
