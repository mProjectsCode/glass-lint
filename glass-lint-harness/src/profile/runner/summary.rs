use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::profile::{
    config::{ProfileConfig, ProfileCorpusIdentity, ProfileWorkload, ProfileWorkloadIdentity},
    runner::files::PreparedCorpus,
    types::{
        MeasuredRepetitionAccumulator, ProfilePhaseTimings, ProfileSummary,
        ProfileSummaryAccumulator, ProfileSummaryMetadata, ProfileWorkloadSummary,
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
    let operation_counts = measured.operation_counts();
    let mut phase_timings = ProfilePhaseTimings::default();
    phase_timings.record_discovery(corpus.setup_duration);
    phase_timings.record_matching(lint_elapsed);
    phase_timings.record_total(total_start.elapsed());
    totals.finish(ProfileSummaryMetadata {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::Files,
            corpus: corpus.manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
            execution: config.execution_identity(),
        },
        setup_duration: corpus.setup_duration,
        measured_elapsed: lint_elapsed,
        wall_duration: total_start.elapsed(),
        repetitions: measured.into_repetitions(),
        phase_timings,
        operation_counts,
    })
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
