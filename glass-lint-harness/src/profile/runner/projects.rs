use std::{path::Path, time::Instant};

use anyhow::Result;
use glass_lint_project::{ProjectLoader, ProjectSelection, ValidatedProjectLoadOptions};

use crate::profile::{
    config::{ProfileConfig, ProfileCorpusIdentity, ProfileWorkload, ProfileWorkloadIdentity},
    runner::support,
    types::{
        MeasuredRepetitionAccumulator, ProfileLinter, ProfileOperationCounts, ProfilePhaseTimings,
        ProfileProjectRun, ProfileProjectRunAccumulator, ProfileSummary, ProfileSummaryAccumulator,
        ProfileSummaryMetadata, project_run_outcome,
    },
};

pub(super) fn run(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let (_, manifest_digest, _) = support::selected_profile_paths(config)?;
    let linters = support::build_linters(config.provider, config.mode, &config.rules)?;
    let loader = ProjectLoader::new(ValidatedProjectLoadOptions::default());
    let mut totals = ProfileSummaryAccumulator::default();
    let mut phases = ProfilePhaseTimings::default();
    let mut counts = ProfileOperationCounts::default();
    let mut measured = MeasuredRepetitionAccumulator::with_repetitions(config.repeat.get());

    for path in &config.paths {
        let project = profile_loader_project(path, config, &loader, &linters)?;
        for (target, source) in measured.repetitions.iter_mut().zip(project.repetitions) {
            target.merge(source);
        }
        phases += project.phases;
        counts += project.counts;
        totals.record(project.result, project.successful_runs);
    }
    phases.record_total(total_start.elapsed());
    Ok(totals.finish(ProfileSummaryMetadata {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::LoaderProject,
            corpus: manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        setup_duration: phases.discovery() + phases.reads(),
        measured_elapsed: phases.parse_and_local_analysis()
            + phases.resolution()
            + phases.linking_and_matching(),
        wall_duration: phases.total(),
        repetitions: measured.repetitions,
        phase_timings: phases,
        operation_counts: counts,
    }))
}

fn profile_loader_project(
    path: &Path,
    config: &ProfileConfig,
    loader: &ProjectLoader,
    linters: &[ProfileLinter],
) -> Result<ProfileProjectRun> {
    let selection = if path.is_dir() {
        ProjectSelection::directory(path.to_owned())
    } else {
        ProjectSelection::entry(path.to_owned())
    };
    let started = Instant::now();
    let mut project = ProfileProjectRunAccumulator::new(path.to_owned(), config.repeat.get());

    for iteration in 0..config.warm_up + config.repeat.get() {
        let repetition_start = Instant::now();
        for ProfileLinter(linter) in linters {
            match loader.load_and_lint(linter, &selection) {
                Ok(outcome) => {
                    let (report, metrics, error) = support::profile_project_parts(outcome);
                    if let Some(error) = error {
                        project.record_error(error);
                    }
                    let outcome = project_run_outcome(&report, &metrics);
                    project.record_success(iteration, config.warm_up, &report, &outcome);
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    project.record_error(error);
                    if !config.continue_on_error {
                        return Err(anyhow::anyhow!(message));
                    }
                }
            }
        }
        if iteration >= config.warm_up {
            project
                .record_repetition_duration(iteration - config.warm_up, repetition_start.elapsed());
        }
    }
    Ok(project.finish(started))
}
