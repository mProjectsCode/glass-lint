//! Job execution runtime for parallel local lowering.
//!
//! Owns the worker-pool dispatch, executor abstraction, and observer hooks.
//! This module contains no phase-state types.

use std::num::NonZeroUsize;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::{
    ParseDiagnostic,
    analysis::{ArtifactCacheKey, LoweredSource, Lowerer},
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
    pub(super) result: Result<LoweredSource, ParseDiagnostic>,
}

pub(super) trait LocalJobCallbacks {
    fn prepare(&mut self, candidate: LocalJobCandidate) -> Option<LocalJob>;
    fn release(&mut self, result: LocalJobResult);
}

pub(super) trait LocalJobExecutor {
    fn execute(
        &self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
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
    LowerAttempted,
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
    lower_attempts: AtomicUsize,
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
            lower_attempts: AtomicUsize::new(0),
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
pub struct InvocationCounts {
    pub parses: usize,
    pub lowers: usize,
    pub hits: usize,
    pub misses: usize,
    pub inserts: usize,
    pub evictions: usize,
}

pub(super) struct ThreadLocalJobExecutor;

impl LocalJobExecutor for ThreadLocalJobExecutor {
    fn execute(
        &self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
        observer: &dyn ExecutionObserver,
        callbacks: &mut dyn LocalJobCallbacks,
    ) -> Result<(), LocalExecutionError> {
        let worker_count = worker_limit.get().max(1);
        let bound = outstanding_job_bound(worker_limit);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .build()
            .map_err(|_| LocalExecutionError::WorkerPanic)?;
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
            let results = pool.install(|| {
                batch
                    .into_par_iter()
                    .map(|job| {
                        observer.observe(ExecutionEvent::Started);
                        observer.observe(ExecutionEvent::ParseAttempted);
                        observer.observe(ExecutionEvent::LowerAttempted);
                        let result = lowerer.lower_source(&job.source);
                        observer.observe(ExecutionEvent::Finished);
                        LocalJobResult {
                            path: job.path,
                            key: job.key,
                            result,
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for result in results {
                callbacks.release(result);
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
        &self,
        candidates: &mut dyn Iterator<Item = LocalJobCandidate>,
        _worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
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
            observer.observe(ExecutionEvent::LowerAttempted);
            let result = lowerer.lower_source(&job.source);
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
    worker_limit.get().saturating_mul(2)
}
