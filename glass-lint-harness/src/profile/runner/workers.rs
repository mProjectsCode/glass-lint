use std::{
    sync::{
        Arc, Barrier, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use glass_lint_core::{
    Linter,
    project::{AnalysisOperationCounts, ReportCompletion, SourceFile},
};

use crate::profile::{
    metrics::{accumulate_report, combined_digest},
    types::{PreparedFile, ProfileWorkloadSummary},
};

fn profile_file(
    file: &PreparedFile,
    linters: &[Arc<Linter>],
    warm_up: usize,
    repeat: usize,
) -> ProfileWorkloadSummary {
    let mut findings = 0;
    let mut diagnostics = 0;
    let mut elapsed = Duration::ZERO;
    let mut completion = ReportCompletion::Complete;
    let mut run_completions = Vec::new();
    let mut operation_counts = AnalysisOperationCounts::default();
    let mut evidence_digests = Vec::new();
    for iteration in 0..warm_up + repeat {
        for linter in linters {
            let started = Instant::now();
            let filename = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("snippet.js");
            let report = linter
                .lint_source(
                    SourceFile::with_language(
                        filename,
                        file.source.clone(),
                        super::support::source_language(&file.path),
                    )
                    .expect("profile paths are valid snippet project identities"),
                )
                .expect("profile paths are valid snippet project identities");
            if iteration >= warm_up {
                elapsed += started.elapsed();
                accumulate_report(
                    &report,
                    &mut findings,
                    &mut diagnostics,
                    &mut operation_counts,
                    &mut evidence_digests,
                );
                completion = completion.join(report.completion());
                run_completions.push(report.completion());
            }
        }
    }
    ProfileWorkloadSummary {
        path: file.path.clone(),
        bytes: file.bytes,
        findings,
        diagnostics,
        measured_elapsed: elapsed,
        completion,
        run_completions,
        operation_counts,
        evidence_order_digest: combined_digest(&evidence_digests),
        error: None,
    }
}

pub(super) fn execute_file_profile(
    prepared: &[PreparedFile],
    linters: &[Arc<Linter>],
    workers: usize,
    warm_up: usize,
    repeat: usize,
) -> (Vec<ProfileWorkloadSummary>, Duration) {
    let warm_up_next = Arc::new(AtomicUsize::new(0));
    let measured_next = Arc::new(AtomicUsize::new(0));
    let warm_up_barrier = Arc::new(Barrier::new(workers));
    let measured_start = Arc::new(OnceLock::new());
    let mut results = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let warm_up_next = Arc::clone(&warm_up_next);
            let measured_next = Arc::clone(&measured_next);
            let warm_up_barrier = Arc::clone(&warm_up_barrier);
            let measured_start = Arc::clone(&measured_start);
            handles.push(scope.spawn(move || {
                let mut results = Vec::new();
                loop {
                    let index = warm_up_next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = prepared.get(index) else {
                        break;
                    };
                    let _ = profile_file(file, linters, warm_up, 0);
                }
                warm_up_barrier.wait();
                measured_start.get_or_init(Instant::now);
                loop {
                    let index = measured_next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = prepared.get(index) else {
                        break;
                    };
                    results.push(profile_file(file, linters, 0, repeat));
                }
                results
            }));
        }

        let mut results = Vec::with_capacity(prepared.len());
        for handle in handles {
            results.extend(handle.join().expect("profile worker panicked"));
        }
        results
    });
    let elapsed = measured_start
        .get()
        .map_or(Duration::ZERO, Instant::elapsed);
    results.sort_by(|left, right| left.path.cmp(&right.path));
    (results, elapsed)
}
