//! Job execution runtime for parallel local lowering.
//!
//! Owns the worker-pool dispatch, executor abstraction, and observer hooks.
//! This module contains no phase-state types.

use std::num::NonZeroUsize;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    LocalExecutionError, ParseDiagnostic, ProjectRelativePath, SourceFile,
    analysis::{ArtifactCacheKey, LoweredSource, Lowerer},
};

pub(super) struct LocalJob {
    pub(super) path: ProjectRelativePath,
    pub(super) source: SourceFile,
    pub(super) key: ArtifactCacheKey,
}

pub(super) struct LocalJobResult {
    pub(super) path: ProjectRelativePath,
    pub(super) key: ArtifactCacheKey,
    pub(super) result: Result<LoweredSource, ParseDiagnostic>,
}

pub(super) trait LocalJobExecutor {
    fn execute(
        &self,
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
        observer: &dyn ExecutionObserver,
        release: &mut dyn FnMut(LocalJobResult),
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
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
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
                        let result = lowerer.lower_source(&job.source);
                        observer.observe(ExecutionEvent::Finished);
                        result_tx
                            .send(LocalJobResult {
                                path: job.path,
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
pub enum ControlledReleaseOrder {
    Forward,
    Reverse,
    Interleaved,
}

#[cfg(test)]
pub struct ControlledLocalJobExecutor(pub ControlledReleaseOrder);

#[cfg(test)]
impl LocalJobExecutor for ControlledLocalJobExecutor {
    fn execute(
        &self,
        jobs: Box<dyn Iterator<Item = LocalJob>>,
        _worker_limit: NonZeroUsize,
        lowerer: &Lowerer,
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
            let result = lowerer.lower_source(&job.source);
            observer.observe(ExecutionEvent::Finished);
            release(LocalJobResult {
                path: job.path,
                key: job.key,
                result,
            });
        }
        Ok(())
    }
}

pub(super) fn normalize_worker_limit(requested: usize) -> NonZeroUsize {
    NonZeroUsize::new(requested).unwrap_or(NonZeroUsize::MIN)
}

pub const fn outstanding_job_bound(worker_limit: NonZeroUsize) -> usize {
    worker_limit.get().saturating_mul(2)
}
