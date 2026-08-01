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

use crate::project::{
    AnalysisReport, LocalExecutionError, ProjectInputError, ProjectRelativePath, SourceFile,
};

/// Configuration for a bounded batch lint operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchOptions {
    workers: NonZeroUsize,
    max_in_flight: NonZeroUsize,
}

impl BatchOptions {
    /// Create options with the requested worker count and the default window.
    pub fn new(workers: NonZeroUsize) -> Self {
        let max_in_flight = workers.get().saturating_mul(2).max(1);
        Self {
            workers,
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
        Self::new(std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN))
    }
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
    result: Result<AnalysisReport, ProjectInputError>,
}

impl BatchResult {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn result(&self) -> &Result<AnalysisReport, ProjectInputError> {
        &self.result
    }

    pub fn into_result(self) -> Result<AnalysisReport, ProjectInputError> {
        self.result
    }
}

pub(super) struct CompletedBatch {
    index: usize,
    path: ProjectRelativePath,
    result: Result<AnalysisReport, ProjectInputError>,
}

struct PendingBatch {
    next_index: usize,
    next_expected: usize,
    in_flight: usize,
    max_in_flight: usize,
    paths: BTreeMap<usize, ProjectRelativePath>,
    completed: BTreeMap<usize, CompletedBatch>,
}

impl PendingBatch {
    fn new(max_in_flight: NonZeroUsize) -> Self {
        Self {
            next_index: 0,
            next_expected: 0,
            in_flight: 0,
            max_in_flight: max_in_flight.get(),
            paths: BTreeMap::new(),
            completed: BTreeMap::new(),
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
        self.paths.insert(index, path);
        index
    }

    fn complete(&mut self, completed: CompletedBatch) {
        debug_assert!(self.paths.contains_key(&completed.index));
        self.completed.insert(completed.index, completed);
    }

    fn synthesize_missing(&mut self) {
        let missing = self
            .paths
            .iter()
            .filter(|(index, _)| !self.completed.contains_key(index))
            .map(|(index, path)| (*index, path.clone()))
            .collect::<Vec<_>>();
        for (index, path) in missing {
            self.completed.insert(
                index,
                CompletedBatch {
                    index,
                    path,
                    result: Err(ProjectInputError::LocalExecution(
                        LocalExecutionError::WorkerPanic,
                    )),
                },
            );
        }
    }

    fn take_ready(&mut self) -> Option<BatchResult> {
        let completed = self.completed.remove(&self.next_expected)?;
        let path = self
            .paths
            .remove(&self.next_expected)
            .expect("every completed batch item was submitted");
        debug_assert_eq!(path, completed.path);
        self.next_expected = self.next_expected.saturating_add(1);
        self.in_flight -= 1;
        Some(BatchResult {
            index: completed.index,
            path,
            result: completed.result,
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

/// Input-ordered results from [`crate::Linter::lint_batch`].
pub struct BatchResults<I>
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
    finished: bool,
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
            input,
            linter,
            pool,
            sender: Some(channel.0),
            receiver: channel.1,
            cancellation,
            pending: PendingBatch::new(max_in_flight),
            exhausted: false,
            finished: false,
        }
    }

    fn refill(&mut self, linter: &crate::Linter, sender: &mpsc::Sender<CompletedBatch>) {
        while self.pending.can_submit() && !self.exhausted {
            let Some(source) = self.input.next() else {
                self.exhausted = true;
                break;
            };
            let path = source.path().clone();
            let index = self.pending.submit(path.clone());
            let cancellation = Arc::clone(&self.cancellation);
            let sender = sender.clone();
            let linter = linter.clone();
            self.pool.spawn(move || {
                if cancellation.load(Ordering::Acquire) {
                    return;
                }
                let result = catch_unwind(AssertUnwindSafe(|| linter.lint_source(source)))
                    .unwrap_or({
                        Err(ProjectInputError::LocalExecution(
                            LocalExecutionError::WorkerPanic,
                        ))
                    });
                let _ = sender.send(CompletedBatch {
                    index,
                    path,
                    result,
                });
            });
        }
    }
}

impl<I> Iterator for BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    type Item = BatchResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let linter = self.linter.clone();
        if let Some(sender) = self.sender.clone() {
            self.refill(&linter, &sender);
        }
        if self.exhausted {
            self.sender.take();
        }
        loop {
            if let Some(result) = self.pending.take_ready() {
                return Some(result);
            }
            if self.pending.in_flight() == 0 {
                self.finished = true;
                return None;
            }
            match self.receiver.recv() {
                Ok(completed) => self.pending.complete(completed),
                Err(_) => self.pending.synthesize_missing(),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pending.size_hint(&self.input)
    }
}

impl<I> std::iter::FusedIterator for BatchResults<I> where I: Iterator<Item = SourceFile> {}

impl<I> Drop for BatchResults<I>
where
    I: Iterator<Item = SourceFile>,
{
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> ProjectRelativePath {
        ProjectRelativePath::new(name).unwrap()
    }

    fn completed(index: usize, name: &str) -> CompletedBatch {
        CompletedBatch {
            index,
            path: path(name),
            result: Err(ProjectInputError::InvalidPath(name.to_owned())),
        }
    }

    #[test]
    fn options_have_non_zero_bounded_defaults() {
        let options = BatchOptions::default();
        assert!(options.workers().get() > 0);
        assert!(options.max_in_flight().get() >= options.workers().get());
        assert_eq!(
            BatchOptions::new(NonZeroUsize::new(usize::MAX).unwrap()).max_in_flight(),
            NonZeroUsize::new(usize::MAX).unwrap()
        );
    }

    #[test]
    fn pending_batch_reorders_completions_and_counts_until_yield() {
        let mut pending = PendingBatch::new(NonZeroUsize::new(3).unwrap());
        for name in ["a.js", "b.js", "c.js"] {
            pending.submit(path(name));
        }
        pending.complete(completed(2, "c.js"));
        pending.complete(completed(0, "a.js"));
        pending.complete(completed(1, "b.js"));
        assert_eq!(pending.in_flight(), 3);
        assert_eq!(pending.take_ready().unwrap().index(), 0);
        assert_eq!(pending.in_flight(), 2);
        assert_eq!(pending.take_ready().unwrap().index(), 1);
        assert_eq!(pending.take_ready().unwrap().index(), 2);
        assert_eq!(pending.in_flight(), 0);
    }

    #[test]
    fn pending_batch_synthesizes_missing_worker_failures() {
        let mut pending = PendingBatch::new(NonZeroUsize::new(2).unwrap());
        pending.submit(path("a.js"));
        pending.submit(path("b.js"));
        pending.complete(completed(1, "b.js"));
        pending.synthesize_missing();
        assert!(matches!(
            pending.take_ready().unwrap().into_result(),
            Err(ProjectInputError::LocalExecution(
                LocalExecutionError::WorkerPanic
            ))
        ));
        assert_eq!(pending.take_ready().unwrap().path().as_str(), "b.js");
    }

    #[test]
    fn pending_size_hint_includes_submitted_results() {
        let mut pending = PendingBatch::new(NonZeroUsize::new(2).unwrap());
        pending.submit(path("a.js"));
        assert_eq!(pending.size_hint(&[1, 2, 3].into_iter()), (4, Some(4)));
    }
}
