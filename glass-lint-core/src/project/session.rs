//! Deterministic project admission, local analysis, and staging.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeMap, num::NonZeroUsize};

struct LocalJob {
    path: super::ProjectRelativePath,
    source: SourceFile,
    key: ArtifactCacheKey,
}

struct LocalJobResult {
    path: super::ProjectRelativePath,
    source: SourceFile,
    key: ArtifactCacheKey,
    result: Result<crate::analysis::SemanticArtifact, crate::ParseDiagnostic>,
}

trait LocalJobExecutor {
    fn execute(
        &self,
        jobs: Vec<LocalJob>,
        worker_limit: NonZeroUsize,
        linter: &crate::Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    );
}

trait ExecutionObserver: Send + Sync {
    fn submitted(&self) {}
    fn started(&self) {}
    fn finished(&self) {}
    fn merged(&self) {}
    fn parse_attempted(&self) {}
    fn lower_attempted(&self) {}
    fn cache_hit(&self) {}
    fn cache_miss(&self) {}
    fn cache_inserted(&self) {}
    fn cache_evicted(&self) {}
}

struct NoopExecutionObserver;
impl ExecutionObserver for NoopExecutionObserver {}

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
    fn submitted(&self) {
        let value = self.outstanding.fetch_add(1, Ordering::SeqCst) + 1;
        Self::peak(&self.peak_outstanding, value);
    }

    fn started(&self) {
        let value = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        Self::peak(&self.peak_active, value);
    }

    fn finished(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn merged(&self) {
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
    }

    fn parse_attempted(&self) {
        self.parse_attempts.fetch_add(1, Ordering::SeqCst);
    }

    fn lower_attempted(&self) {
        self.lower_attempts.fetch_add(1, Ordering::SeqCst);
    }

    fn cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::SeqCst);
    }

    fn cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::SeqCst);
    }

    fn cache_inserted(&self) {
        self.cache_inserts.fetch_add(1, Ordering::SeqCst);
    }

    fn cache_evicted(&self) {
        self.cache_evictions.fetch_add(1, Ordering::SeqCst);
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
        linter: &crate::Linter,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
    ) {
        let bound = outstanding_job_bound(worker_limit);
        for batch in jobs.chunks(bound) {
            for _ in batch {
                observer.submitted();
            }
            let batch_results = std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for chunk in batch.chunks(batch.len().max(1).div_ceil(worker_limit.get())) {
                    handles.push(scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|job| LocalJobResult {
                                path: job.path.clone(),
                                source: job.source.clone(),
                                key: job.key.clone(),
                                result: {
                                    observer.started();
                                    observer.parse_attempted();
                                    observer.lower_attempted();
                                    let result =
                                        crate::analysis::lower_artifact(linter, &job.source);
                                    observer.finished();
                                    result
                                },
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
        linter: &crate::Linter,
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
            observer.submitted();
            observer.started();
            observer.parse_attempted();
            observer.lower_attempted();
            let result = crate::analysis::lower_artifact(linter, &job.source);
            observer.finished();
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

pub(super) const fn outstanding_job_bound(worker_limit: NonZeroUsize) -> usize {
    worker_limit.get().saturating_mul(2)
}

use super::{
    AnalysisReport, ProjectInput, ProjectInputError, ResolutionRequest, ResolutionRequestKey,
    ResolverOutcome, SourceFile,
    input::{normalize_relative, normalize_resolution_key, normalize_result, normalize_root},
    tables::{ResolutionTable, SourceTable},
};
use crate::analysis::{
    ArtifactCacheHandle, ArtifactCacheKey, LocatedSourceContext, LoweredSource,
    SharedSemanticArtifact,
};

pub struct AnalysisSession<'a> {
    pub(super) linter: &'a crate::Linter,
    pub(super) root: std::path::PathBuf,
    pub(super) sources: SourceTable,
    pub(super) resolutions: ResolutionTable,
    pub(super) authored_requests: BTreeMap<ResolutionRequestKey, ResolutionRequest>,
    pub(super) analyzed: BTreeMap<super::ProjectRelativePath, crate::analysis::LocalArtifact>,
    pub(super) parse_diagnostics: BTreeMap<super::ProjectRelativePath, crate::ParseDiagnostic>,
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

    /// Start an empty parse-once project session under a canonical root.
    pub fn new(
        linter: &'a crate::Linter,
        root: impl Into<std::path::PathBuf>,
    ) -> Result<Self, ProjectInputError> {
        Ok(Self {
            linter,
            root: normalize_root(&root.into())?,
            sources: SourceTable::default(),
            resolutions: ResolutionTable::default(),
            authored_requests: BTreeMap::new(),
            analyzed: BTreeMap::new(),
            parse_diagnostics: BTreeMap::new(),
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
        let source = self
            .sources
            .get(path.as_str())
            .ok_or_else(|| ProjectInputError::InvalidPath(path.to_string()))?;
        let key = self.artifact_fingerprint(source);
        let lowered = if let Some(cached) = self.artifact_cache.get(&key) {
            observer.cache_hit();
            LoweredSource {
                source: LocatedSourceContext::new(source),
                semantic: (*cached.semantic).clone(),
            }
        } else {
            observer.cache_miss();
            observer.parse_attempted();
            observer.lower_attempted();
            let lowered = match crate::analysis::lower_source(self.linter, source) {
                Ok(lowered) => lowered,
                Err(error) => {
                    self.parse_diagnostics.insert(path.clone(), error);
                    return Ok(Vec::new());
                }
            };
            let evicted = self.artifact_cache.insert(
                key,
                SharedSemanticArtifact {
                    semantic: std::sync::Arc::new(lowered.semantic.clone()),
                },
            );
            observer.cache_inserted();
            if evicted {
                observer.cache_evicted();
            }
            lowered
        };
        Ok(self.record_lowered(&path, lowered))
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
        path: &super::ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        Self::record_lowered_into(
            &mut self.authored_requests,
            &mut self.analyzed,
            path,
            lowered,
        )
    }

    fn record_lowered_into(
        authored_requests: &mut BTreeMap<ResolutionRequestKey, ResolutionRequest>,
        analyzed: &mut BTreeMap<super::ProjectRelativePath, crate::analysis::LocalArtifact>,
        path: &super::ProjectRelativePath,
        lowered: LoweredSource,
    ) -> Vec<ResolutionRequest> {
        let local = crate::analysis::LocalArtifact::new(
            lowered.source.clone(),
            std::sync::Arc::new(lowered.semantic),
        );
        let requests = local.interface().authored_requests(
            path,
            &local.source_context().lines,
            &local.source_context().text,
        );
        for request in &requests {
            authored_requests.insert(request.key.clone(), request.clone());
        }
        analyzed.insert(path.clone(), local);
        requests
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
                !self.analyzed.contains_key(*path) && !self.parse_diagnostics.contains_key(*path)
            })
            .map(|(path, source)| (path.to_owned(), source.clone()))
            .collect::<Vec<_>>();
        let mut requests = Vec::new();
        let mut uncached = Vec::new();
        for (path, source) in pending {
            let key = self.artifact_fingerprint(&source);
            if let Some(cached) = self.artifact_cache.get(&key) {
                observer.cache_hit();
                requests.extend(self.record_lowered(
                    &super::ProjectRelativePath::new(path)?,
                    LoweredSource {
                        source: LocatedSourceContext::new(&source),
                        semantic: (*cached.semantic).clone(),
                    },
                ));
            } else {
                observer.cache_miss();
                uncached.push(LocalJob {
                    path: super::ProjectRelativePath::new(path)?,
                    source,
                    key,
                });
            }
        }

        let artifact_cache = self.artifact_cache.clone();
        let authored_requests = &mut self.authored_requests;
        let analyzed = &mut self.analyzed;
        let parse_diagnostics = &mut self.parse_diagnostics;
        let mut release = |result: LocalJobResult| {
            match result.result {
                Ok(artifact) => {
                    let evicted = artifact_cache.insert(
                        result.key,
                        SharedSemanticArtifact {
                            semantic: std::sync::Arc::new(artifact.clone()),
                        },
                    );
                    observer.cache_inserted();
                    if evicted {
                        observer.cache_evicted();
                    }
                    requests.extend(Self::record_lowered_into(
                        authored_requests,
                        analyzed,
                        &result.path,
                        LoweredSource {
                            source: LocatedSourceContext::new(&result.source),
                            semantic: artifact,
                        },
                    ));
                }
                Err(error) => {
                    parse_diagnostics.insert(result.path, error);
                }
            }
            observer.merged();
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
        self.analyze_source(path)
    }

    /// Record a resolver answer only for an authored module request.
    pub fn record_resolution(
        &mut self,
        mut key: ResolutionRequestKey,
        mut result: ResolverOutcome,
    ) -> Result<(), ProjectInputError> {
        normalize_resolution_key(&mut key)?;
        if !self.authored_requests.contains_key(&key) {
            return Err(ProjectInputError::UnknownRequest(key));
        }
        normalize_result(&mut result)?;
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
        let input = ProjectInput {
            root: self.root,
            sources: self.sources.into_values().collect(),
            resolutions: self.resolutions.into_values().collect(),
        }
        .validate()?;
        self.linter
            .finish_analyzed_project(input, self.analyzed, self.parse_diagnostics)
    }
}
