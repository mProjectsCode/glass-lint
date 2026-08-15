use std::sync::atomic::AtomicUsize;

use super::*;
use crate::{Environment, Linter, LinterConfig, RuleCatalog, project::ProjectInputError};

fn path(name: &str) -> ProjectRelativePath {
    ProjectRelativePath::new(name).unwrap()
}

fn completed(index: usize, name: &str) -> CompletedBatch {
    CompletedBatch {
        index,
        result: Err(ProjectError::Input(ProjectInputError::InvalidPath(
            name.to_owned(),
        ))),
    }
}

#[test]
fn options_have_non_zero_bounded_defaults() {
    let options = BatchOptions::default();
    assert!(options.workers().get() > 0);
    assert!(options.max_in_flight().get() >= options.workers().get());

    let capped = BatchOptions::new(NonZeroUsize::new(usize::MAX).unwrap());
    assert!(capped.workers().get() > 0);
    assert!(capped.max_in_flight().get() >= capped.workers().get());
    assert!(capped.workers().get() <= host_parallelism());
}

#[test]
fn pending_batch_reorders_completions_and_counts_until_yield() {
    let mut pending = PendingBatch::new(NonZeroUsize::new(3).unwrap());
    for name in ["a.js", "b.js", "c.js"] {
        pending.submit(path(name));
    }
    let _ = pending.complete(completed(2, "c.js"));
    let _ = pending.complete(completed(0, "a.js"));
    let _ = pending.complete(completed(1, "b.js"));
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
    let _ = pending.complete(completed(1, "b.js"));
    pending.synthesize_missing();
    assert_eq!(
        pending.take_ready().unwrap().into_result(),
        Err(worker_panic())
    );
    assert_eq!(pending.take_ready().unwrap().path().as_str(), "b.js");
}

#[test]
fn invalid_completion_protocol_fails_all_pending_entries() {
    let mut pending = PendingBatch::new(NonZeroUsize::new(2).unwrap());
    pending.submit(path("a.js"));
    pending.submit(path("b.js"));

    assert!(pending.complete(completed(9, "unknown.js")).is_err());
    pending.fail_protocol();
    for _ in 0..2 {
        assert_eq!(
            pending.take_ready().unwrap().into_result(),
            Err(worker_panic())
        );
    }
    assert_eq!(pending.in_flight(), 0);
}

#[test]
fn duplicate_completion_protocol_fails_without_waiting() {
    let mut pending = PendingBatch::new(NonZeroUsize::new(1).unwrap());
    pending.submit(path("a.js"));
    assert!(pending.complete(completed(0, "a.js")).is_ok());
    assert!(pending.complete(completed(0, "a.js")).is_err());
    pending.fail_protocol();
    assert_eq!(
        pending.take_ready().unwrap().into_result(),
        Err(worker_panic())
    );
}

#[test]
fn protocol_failure_stops_submitting_new_inputs() {
    let linter = Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![]).unwrap()],
        Environment::default(),
    ))
    .unwrap();
    let submitted = Arc::new(AtomicUsize::new(0));
    let input_count = Arc::clone(&submitted);
    let input = std::iter::from_fn(move || {
        let index = input_count.fetch_add(1, Ordering::Relaxed);
        Some(
            crate::project::SourceFile::new(format!("{index}.js"), "")
                .expect("generated test paths are valid"),
        )
    });
    let (sender, receiver) = mpsc::channel();
    sender.send(completed(usize::MAX, "unknown.js")).unwrap();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let cancellation = Arc::new(AtomicBool::new(false));
    let mut driver = BatchDriver::new(
        input,
        linter,
        pool,
        (sender, receiver),
        cancellation,
        NonZeroUsize::new(2).unwrap(),
    );

    assert_eq!(
        driver.next_result().unwrap().into_result(),
        Err(worker_panic())
    );
    assert_eq!(
        driver.next_result().unwrap().into_result(),
        Err(worker_panic())
    );
    assert!(driver.next_result().is_none());
    assert_eq!(submitted.load(Ordering::Relaxed), 2);
}

#[test]
fn pending_size_hint_includes_submitted_results() {
    let mut pending = PendingBatch::new(NonZeroUsize::new(2).unwrap());
    pending.submit(path("a.js"));
    assert_eq!(pending.size_hint(&[1, 2, 3].into_iter()), (4, Some(4)));
}
