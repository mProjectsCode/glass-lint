use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::profile::{
    config::{ProfileConfig, ProfileCorpusIdentity, ProfileWorkload, ProfileWorkloadIdentity},
    metrics::median_duration,
    runner::files::PreparedCorpus,
    types::{
        MeasuredRepetitionAccumulator, ProfilePhaseTimings, ProfileSummary,
        ProfileSummaryAccumulator, ProfileWorkloadSummary, sum_operation_counts,
    },
};

pub(super) fn file_profile(
    config: &ProfileConfig,
    total_start: Instant,
    corpus: PreparedCorpus,
    lint_elapsed: Duration,
    measured_results: Vec<ProfileWorkloadSummary>,
    measured: MeasuredRepetitionAccumulator,
) -> ProfileSummary {
    let mut workload_results = corpus.initial_errors;
    workload_results.extend(aggregate_workload_results(measured_results));
    workload_results.sort_by(|left, right| left.path.cmp(&right.path));

    let mut totals = ProfileSummaryAccumulator::default();
    for result in workload_results {
        totals.record(result, config.repeat.get());
    }
    let operation_counts = sum_operation_counts(&measured.repetitions);
    let mut phase_timings = ProfilePhaseTimings::default();
    phase_timings.record_discovery(corpus.setup_duration);
    phase_timings.record_matching(lint_elapsed);
    phase_timings.record_total(total_start.elapsed());
    ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::Files,
            corpus: corpus.manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        inputs: totals.files,
        bytes: totals.bytes,
        findings: totals.findings,
        diagnostics: totals.diagnostics,
        errors: totals.errors,
        runs: totals.runs,
        setup_duration: corpus.setup_duration,
        measured_elapsed: lint_elapsed,
        wall_duration: total_start.elapsed(),
        median_repetition_duration: median_duration(&measured.repetitions),
        repetitions: measured.repetitions,
        workload_results: totals.workload_results,
        phase_timings,
        operation_counts,
    }
}

fn aggregate_workload_results(results: Vec<ProfileWorkloadSummary>) -> Vec<ProfileWorkloadSummary> {
    let mut aggregated = BTreeMap::<PathBuf, ProfileWorkloadSummary>::new();
    for result in results {
        aggregated
            .entry(result.path.clone())
            .or_insert_with(|| ProfileWorkloadSummary::new(result.path.clone()))
            .merge(result);
    }
    aggregated.into_values().collect()
}
