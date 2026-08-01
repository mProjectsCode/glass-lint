use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use glass_lint_core::{BatchOptions, Environment, Linter, RuleCatalog, project::SourceFile};

use crate::support::{self, rule};

fn linter() -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let rule = rule("network.fetch")
        .query(glass_lint_core::rules::EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    support::linter_from_catalog(RuleCatalog::new("test", vec![rule]).unwrap(), environment)
}

fn source(path: &str, text: &str) -> SourceFile {
    SourceFile::new(path, text).unwrap()
}

#[test]
fn batch_matches_the_canonical_single_source_report() {
    let linter = linter();
    let input = source("main.js", "fetch('/data');");
    let single = linter.lint_source(input.clone()).unwrap();
    let batch = linter
        .lint_batch([input], BatchOptions::new(NonZeroUsize::MIN))
        .unwrap()
        .next()
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(single, batch);
}

#[test]
fn batch_is_lazy_bounded_and_input_ordered() {
    let pulled = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&pulled);
    let inputs = (0..5).map(move |index| {
        count.fetch_add(1, Ordering::SeqCst);
        source(&format!("{index}.js"), "fetch('/data');")
    });
    let mut results = linter()
        .lint_batch(
            inputs,
            BatchOptions::new(NonZeroUsize::new(2).unwrap())
                .with_max_in_flight(NonZeroUsize::new(2).unwrap()),
        )
        .unwrap();
    assert_eq!(pulled.load(Ordering::SeqCst), 0);
    assert_eq!(results.size_hint().0, 5);
    let first = results.next().unwrap();
    assert_eq!(first.index(), 0);
    assert_eq!(pulled.load(Ordering::SeqCst), 2);
    let indexes = results.map(|result| result.index()).collect::<Vec<_>>();
    assert_eq!(indexes, vec![1, 2, 3, 4]);
    assert_eq!(pulled.load(Ordering::SeqCst), 5);
}

#[test]
fn duplicate_paths_are_independent_batch_items() {
    let results = linter()
        .lint_batch(
            [
                source("same.js", "fetch('/one');"),
                source("same.js", "fetch('/two');"),
            ],
            BatchOptions::new(NonZeroUsize::new(2).unwrap()),
        )
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].index(), 0);
    assert_eq!(results[1].index(), 1);
    assert!(results.iter().all(|result| result.result().is_ok()));
}

#[test]
fn malformed_item_does_not_stop_later_batch_results() {
    let results = linter()
        .lint_batch(
            [
                source("broken.js", "fetch("),
                source("valid.js", "fetch('/ok');"),
            ],
            BatchOptions::new(NonZeroUsize::MIN),
        )
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 2);
    assert!(results[0].result().is_ok());
    assert!(results[0].result().as_ref().unwrap().files()[0].has_parse_diagnostics());
    assert_eq!(
        results[1].result().as_ref().unwrap().files()[0]
            .findings()
            .len(),
        1
    );
}

#[test]
fn empty_batch_is_fused_and_dropping_stops_at_the_window() {
    let mut empty = linter()
        .lint_batch(
            std::iter::empty::<SourceFile>(),
            BatchOptions::new(NonZeroUsize::MIN),
        )
        .unwrap();
    assert!(empty.next().is_none());
    assert!(empty.next().is_none());

    let pulled = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&pulled);
    let inputs = (0..10).map(move |index| {
        count.fetch_add(1, Ordering::SeqCst);
        source(&format!("{index}.js"), "fetch('/data');")
    });
    let mut results = linter()
        .lint_batch(
            inputs,
            BatchOptions::new(NonZeroUsize::new(4).unwrap())
                .with_max_in_flight(NonZeroUsize::new(2).unwrap()),
        )
        .unwrap();
    assert!(results.next().is_some());
    assert_eq!(pulled.load(Ordering::SeqCst), 2);
    drop(results);
    assert_eq!(pulled.load(Ordering::SeqCst), 2);
}

#[test]
fn cache_reuse_keeps_sequential_duplicate_reports_identical() {
    let input = source("same.js", "fetch('/data');");
    let reports = linter()
        .lint_batch(
            [input.clone(), input],
            BatchOptions::new(NonZeroUsize::new(2).unwrap()).with_max_in_flight(NonZeroUsize::MIN),
        )
        .unwrap()
        .map(|result| result.into_result().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(reports[0], reports[1]);
}
