//! Job execution runtime for parallel local analysis.
//!
//! Owns the worker-pool dispatch, executor abstraction, and observer hooks.
//! This module contains no phase-state types.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
};

use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};

use crate::{
    ParseDiagnostic,
    analysis::{AnalyzedSource, ArtifactCacheKey, SemanticAnalyzer},
    project::{LocalExecutionError, ProjectRelativePath, SourceFile},
};

pub(super) struct LocalJob {
    pub(super) path: ProjectRelativePath,
    pub(super) source: SourceFile,
    pub(super) key: ArtifactCacheKey,
}

pub(super) struct LocalJobCandidate {
    pub(super) path: ProjectRelativePath,
    pub(super) source: SourceFile,
}

pub(super) struct LocalJobResult {
    pub(super) path: ProjectRelativePath,
    pub(super) key: ArtifactCacheKey,
    pub(super) result: Result<AnalyzedSource, ParseDiagnostic>,
}

enum LocalJobOutcome {
    Completed(LocalJobResult),
    Panicked(LocalJob),
}

pub(super) trait LocalJobCallbacks {
    fn prepare(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob>;
    fn release(&mut self, result: LocalJobResult);
    fn discard(&mut self, job: LocalJob);
}

pub(super) trait LocalJobExecutor {
    fn execute(
        &mut self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        worker_limit: NonZeroUsize,
        analyzer: &SemanticAnalyzer,
        observer: &dyn ExecutionObserver,
        callbacks: &mut dyn LocalJobCallbacks,
    ) -> Result<(), LocalExecutionError>;
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ExecutionEvent {
    Submitted,
    Started,
    Finished,
    Merged,
    ParseAttempted,
    AnalysisAttempted,
    CacheHit,
    CacheMiss,
    CacheInserted,
    CacheEvicted,
}

pub(super) trait ExecutionObserver: Send + Sync {
    fn observe(&self, event: ExecutionEvent);
}

pub(super) struct NoopExecutionObserver;
impl ExecutionObserver for NoopExecutionObserver {
    fn observe(&self, _event: ExecutionEvent) {}
}

#[cfg(test)]
pub struct CountingExecutionObserver {
    active: AtomicUsize,
    peak_active: AtomicUsize,
    outstanding: AtomicUsize,
    peak_outstanding: AtomicUsize,
    parse_attempts: AtomicUsize,
    analysis_attempts: AtomicUsize,
    cache_hits: AtomicUsize,
    cache_misses: AtomicUsize,
    cache_inserts: AtomicUsize,
    cache_evictions: AtomicUsize,
}

#[cfg(test)]
impl CountingExecutionObserver {
    pub fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            peak_active: AtomicUsize::new(0),
            outstanding: AtomicUsize::new(0),
            peak_outstanding: AtomicUsize::new(0),
            parse_attempts: AtomicUsize::new(0),
            analysis_attempts: AtomicUsize::new(0),
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            cache_inserts: AtomicUsize::new(0),
            cache_evictions: AtomicUsize::new(0),
        }
    }

    pub fn peaks(&self) -> (usize, usize) {
        (
            self.peak_active.load(Ordering::SeqCst),
            self.peak_outstanding.load(Ordering::SeqCst),
        )
    }

    pub fn invocations(&self) -> InvocationCounts {
        InvocationCounts {
            parses: self.parse_attempts.load(Ordering::SeqCst),
            analyses: self.analysis_attempts.load(Ordering::SeqCst),
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
            ExecutionEvent::AnalysisAttempted => {
                self.analysis_attempts.fetch_add(1, Ordering::SeqCst);
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
pub struct InvocationCounts {
    pub parses: usize,
    pub analyses: usize,
    pub hits: usize,
    pub misses: usize,
    pub inserts: usize,
    pub evictions: usize,
}

pub(super) struct ThreadLocalJobExecutor {
    pool: Option<WorkerPool>,
}

struct WorkerPool {
    worker_count: usize,
    pool: ThreadPool,
}

impl ThreadLocalJobExecutor {
    pub(super) const fn new() -> Self {
        Self { pool: None }
    }

    fn pool(&mut self, worker_limit: NonZeroUsize) -> Result<&ThreadPool, LocalExecutionError> {
        let worker_count = worker_limit.get().max(1);
        let needs_rebuild = self
            .pool
            .as_ref()
            .is_none_or(|pool| pool.worker_count != worker_count);
        if needs_rebuild {
            let pool = ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .build()
                .map_err(|_| LocalExecutionError::WorkerPanic)?;
            self.pool = Some(WorkerPool { worker_count, pool });
        }
        Ok(&self
            .pool
            .as_ref()
            .expect("executor pool is initialized")
            .pool)
    }
}

impl LocalJobExecutor for ThreadLocalJobExecutor {
    fn execute(
        &mut self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        worker_limit: NonZeroUsize,
        analyzer: &SemanticAnalyzer,
        observer: &dyn ExecutionObserver,
        callbacks: &mut dyn LocalJobCallbacks,
    ) -> Result<(), LocalExecutionError> {
        let bound = outstanding_job_bound(worker_limit);
        let pool = self.pool(worker_limit)?;
        let mut exhausted = false;
        while !exhausted {
            let mut batch = Vec::with_capacity(bound);
            while batch.len() < bound {
                let Some(candidate) = candidates.next() else {
                    exhausted = true;
                    break;
                };
                if let Some(job) = callbacks.prepare(candidate) {
                    batch.push(job);
                }
            }
            if batch.is_empty() {
                continue;
            }
            for _ in &batch {
                observer.observe(ExecutionEvent::Submitted);
            }
            let results = catch_unwind(AssertUnwindSafe(|| {
                pool.install(|| {
                    batch
                        .into_par_iter()
                        .map(|job| {
                            observer.observe(ExecutionEvent::Started);
                            observer.observe(ExecutionEvent::ParseAttempted);
                            observer.observe(ExecutionEvent::AnalysisAttempted);
                            let result = catch_unwind(AssertUnwindSafe(|| {
                                analyzer.analyze_source(&job.source)
                            }));
                            observer.observe(ExecutionEvent::Finished);
                            match result {
                                Ok(result) => LocalJobOutcome::Completed(LocalJobResult {
                                    path: job.path,
                                    key: job.key,
                                    result,
                                }),
                                Err(_) => LocalJobOutcome::Panicked(job),
                            }
                        })
                        .collect::<Vec<_>>()
                })
            }))
            .map_err(|_| LocalExecutionError::WorkerPanic)?;
            let mut worker_panicked = false;
            for result in results {
                match result {
                    LocalJobOutcome::Completed(result) => callbacks.release(result),
                    LocalJobOutcome::Panicked(job) => {
                        callbacks.discard(job);
                        worker_panicked = true;
                    }
                }
            }
            if worker_panicked {
                return Err(LocalExecutionError::WorkerPanic);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "serde"), allow(dead_code))]
pub enum ControlledReleaseOrder {
    Forward,
    Reverse,
    Interleaved,
}

#[cfg(test)]
#[cfg_attr(not(feature = "serde"), allow(dead_code))]
pub struct ControlledLocalJobExecutor(pub ControlledReleaseOrder);

#[cfg(test)]
#[cfg_attr(not(feature = "serde"), allow(dead_code))]
impl LocalJobExecutor for ControlledLocalJobExecutor {
    fn execute(
        &mut self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        _worker_limit: NonZeroUsize,
        analyzer: &SemanticAnalyzer,
        observer: &dyn ExecutionObserver,
        callbacks: &mut dyn LocalJobCallbacks,
    ) -> Result<(), LocalExecutionError> {
        let all: Vec<_> = candidates.collect();
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
            let candidate = jobs[index].take().expect("release index is unique");
            let Some(job) = callbacks.prepare(candidate) else {
                continue;
            };
            observer.observe(ExecutionEvent::Submitted);
            observer.observe(ExecutionEvent::Started);
            observer.observe(ExecutionEvent::ParseAttempted);
            observer.observe(ExecutionEvent::AnalysisAttempted);
            let result = analyzer.analyze_source(&job.source);
            observer.observe(ExecutionEvent::Finished);
            callbacks.release(LocalJobResult {
                path: job.path,
                key: job.key,
                result,
            });
        }
        Ok(())
    }
}

pub(super) fn normalize_worker_limit(requested: usize) -> NonZeroUsize {
    let nonzero = NonZeroUsize::new(requested).unwrap_or(NonZeroUsize::MIN);
    let available = std::thread::available_parallelism().map_or(usize::MAX, NonZeroUsize::get);
    // SAFETY: `max(1)` ensures the result is always non-zero.
    NonZeroUsize::new(nonzero.get().min(available).max(1)).expect("capped worker count is non-zero")
}

pub const fn outstanding_job_bound(worker_limit: NonZeroUsize) -> usize {
    crate::bounds::in_flight_window(worker_limit.get())
}
