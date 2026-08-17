//! Bounded, input-ordered linting of independent owned sources.

use std::{
    collections::BTreeMap,
    fmt,
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use rayon::ThreadPool;

use crate::{
    bounds::in_flight_window,
    project::{
        AnalysisReport, LocalExecutionError, ProjectError, ProjectExecutionError,
        ProjectRelativePath, SourceFile,
    },
};

/// Configuration for a bounded batch lint operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchOptions {
    workers: NonZeroUsize,
    max_in_flight: NonZeroUsize,
}

impl BatchOptions {
    /// Create options with the requested worker count capped by host
    /// parallelism, and the default window.
    pub fn new(workers: NonZeroUsize) -> Self {
        Self::from_workers(workers.get().min(host_parallelism()))
    }

    fn from_workers(workers: usize) -> Self {
        let max_in_flight = in_flight_window(workers);
        Self {
            workers: NonZeroUsize::new(workers).unwrap_or(NonZeroUsize::MIN),
            max_in_flight: NonZeroUsize::new(max_in_flight).unwrap_or(NonZeroUsize::MIN),
        }
    }

    /// Set the maximum number of submitted but not-yet-yielded inputs.
    #[must_use]
    pub fn with_max_in_flight(mut self, max_in_flight: NonZeroUsize) -> Self {
        self.max_in_flight = max_in_flight;
        self
    }

    pub fn workers(&self) -> NonZeroUsize {
        self.workers
    }

    pub fn max_in_flight(&self) -> NonZeroUsize {
        self.max_in_flight
    }
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self::from_workers(host_parallelism())
    }
}

fn host_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Failure to create the dedicated worker pool for a batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchStartError {
    WorkerPoolUnavailable,
}

impl fmt::Display for BatchStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerPoolUnavailable => f.write_str("batch worker pool unavailable"),
        }
    }
}

impl std::error::Error for BatchStartError {}

/// The result for one input in a batch.
pub struct BatchResult {
    index: usize,
    path: ProjectRelativePath,
    result: Result<AnalysisReport, ProjectError>,
}

impl BatchResult {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn result(&self) -> &Result<AnalysisReport, ProjectError> {
        &self.result
    }

    pub fn into_result(self) -> Result<AnalysisReport, ProjectError> {
        self.result
    }
}

pub(super) struct CompletedBatch {
    index: usize,
    result: Result<AnalysisReport, ProjectError>,
}

struct PendingEntry {
    path: ProjectRelativePath,
    result: Option<Result<AnalysisReport, ProjectError>>,
}

struct PendingBatch {
    next_index: usize,
    next_expected: usize,
    in_flight: usize,
    max_in_flight: usize,
    entries: BTreeMap<usize, PendingEntry>,
}

fn worker_panic() -> ProjectError {
    ProjectError::Execution(ProjectExecutionError::Local(
        LocalExecutionError::WorkerPanic,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompletionError {
    UnknownIndex,
    DuplicateIndex,
}

impl PendingBatch {
    fn new(max_in_flight: NonZeroUsize) -> Self {
        Self {
            next_index: 0,
            next_expected: 0,
            in_flight: 0,
            max_in_flight: max_in_flight.get(),
            entries: BTreeMap::new(),
        }
    }

    fn can_submit(&self) -> bool {
        self.in_flight < self.max_in_flight
    }

    fn submit(&mut self, path: ProjectRelativePath) -> usize {
        debug_assert!(self.can_submit());
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        self.in_flight += 1;
        self.entries
            .insert(index, PendingEntry { path, result: None });
        index
    }

    fn complete(&mut self, completed: CompletedBatch) -> Result<(), CompletionError> {
        let Some(entry) = self.entries.get_mut(&completed.index) else {
            return Err(CompletionError::UnknownIndex);
        };
        if entry.result.is_some() {
            return Err(CompletionError::DuplicateIndex);
        }
        entry.result = Some(completed.result);
        Ok(())
    }

    fn fail_protocol(&mut self) {
        for entry in self.entries.values_mut() {
            entry.result = Some(Err(worker_panic()));
        }
    }

    fn synthesize_missing(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.result.is_none() {
                entry.result = Some(Err(worker_panic()));
            }
        }
    }

    fn take_ready(&mut self) -> Option<BatchResult> {
        let index = self.next_expected;
        let entry = self.entries.remove(&index)?;
        let Some(result) = entry.result else {
            self.entries.insert(index, entry);
            return None;
        };
        let path = entry.path;
        self.next_expected = self.next_expected.saturating_add(1);
        self.in_flight -= 1;
        Some(BatchResult {
            index,
            path,
            result,
        })
    }

    fn in_flight(&self) -> usize {
        self.in_flight
    }

    fn size_hint<I>(&self, input: &I) -> (usize, Option<usize>)
    where
        I: Iterator,
    {
        let (lower, upper) = input.size_hint();
        (
            lower.saturating_add(self.in_flight),
            upper.map(|upper| upper.saturating_add(self.in_flight)),
        )
    }
}

/// Private batch protocol driver for [`crate::Linter::lint_batch`].
struct BatchDriver<I>
where
    I: Iterator<Item = SourceFile>,
{
    input: I,
    linter: crate::Linter,
    pool: ThreadPool,
    receiver: mpsc::Receiver<CompletedBatch>,
    sender: Option<mpsc::Sender<CompletedBatch>>,
    cancellation: Arc<AtomicBool>,
    pending: PendingBatch,
    exhausted: bool,
    aborted: bool,
    finished: bool,
}

impl<I> BatchDriver<I>
where
    I: Iterator<Item = SourceFile>,
{
    fn new(
        input: I,
        linter: crate::Linter,
        pool: ThreadPool,
        channel: (mpsc::Sender<CompletedBatch>, mpsc::Receiver<CompletedBatch>),
        cancellation: Arc<AtomicBool>,
        max_in_flight: NonZeroUsize,
    ) -> Self {
        Self {
            input,
            linter,
            pool,
            sender: Some(channel.0),
            receiver: channel.1,
            cancellation,
            pending: PendingBatch::new(max_in_flight),
            exhausted: false,
            aborted: false,
            finished: false,
        }
    }

    fn refill(&mut self) {
        while self.pending.can_submit() && !self.exhausted && !self.aborted {
            let Some(source) = self.input.next() else {
                self.exhausted = true;
                break;
            };
            let index = self.pending.submit(source.path().clone());
            let cancellation = Arc::clone(&self.cancellation);
            let Some(sender) = self.sender.clone() else {
                break;
            };
            let linter = self.linter.clone();
            self.pool.spawn(move || {
                if cancellation.load(Ordering::Acquire) {
                    return;
                }
                let result = catch_unwind(AssertUnwindSafe(|| linter.run_single_source(source)))
                    .unwrap_or_else(|_| Err(worker_panic()));
                let _ = sender.send(CompletedBatch { index, result });
            });
        }
    }

    fn close_input(&mut self) {
        if self.exhausted {
            self.sender.take();
        }
    }

    fn receive_completion(&mut self) {
        match self.receiver.recv() {
            Ok(completed) => {
                if self.pending.complete(completed).is_err() {
                    self.pending.fail_protocol();
                    self.sender.take();
                    self.aborted = true;
                }
            }
            Err(_) => self.pending.synthesize_missing(),
        }
    }

    fn next_result(&mut self) -> Option<BatchResult> {
        if self.finished {
            return None;
        }

        self.refill();
        self.close_input();
        loop {
            if let Some(result) = self.pending.take_ready() {
                return Some(result);
            }
            if self.pending.in_flight() == 0 {
                self.finished = true;
                return None;
            }
            self.receive_completion();
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pending.size_hint(&self.input)
    }

    fn cancel(&self) {
        self.cancellation.store(true, Ordering::Release);
    }
}

/// Input-ordered results from [`crate::Linter::lint_batch`].
pub struct BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    driver: BatchDriver<I>,
}

impl<I> BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    pub(super) fn new(
        input: I,
        linter: crate::Linter,
        pool: ThreadPool,
        channel: (mpsc::Sender<CompletedBatch>, mpsc::Receiver<CompletedBatch>),
        cancellation: Arc<AtomicBool>,
        max_in_flight: NonZeroUsize,
    ) -> Self {
        Self {
            driver: BatchDriver::new(input, linter, pool, channel, cancellation, max_in_flight),
        }
    }
}

impl<I> Iterator for BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    type Item = BatchResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.driver.next_result()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.driver.size_hint()
    }
}

impl<I> std::iter::FusedIterator for BatchResults<I> where I: Iterator<Item = SourceFile> {}

impl<I> Drop for BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    fn drop(&mut self) {
        self.driver.cancel();
    }
}

#[cfg(test)]
mod tests;
