use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use glass_lint_core::{Linter, project::ReportCompletion};

use crate::profile::{
    config::ProfileConfig,
    metrics::repetition_from_files,
    runner::{summary, support, workers},
    types::{MeasuredRepetitionAccumulator, PreparedFile, ProfileSummary, ProfileWorkloadSummary},
};

pub(super) fn run(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let corpus = prepare_corpus(config)?;
    let mut measured_results_by_run = Vec::new();
    let measured = MeasuredRepetitionAccumulator::measure(
        config.warm_up,
        config.repeat.get(),
        || {
            let _ = workers::execute_file_profile(
                &corpus.prepared,
                &corpus.linters,
                config.workers.get(),
                1,
                0,
            );
            Ok(())
        },
        || {
            let (results, duration) = workers::execute_file_profile(
                &corpus.prepared,
                &corpus.linters,
                config.workers.get(),
                0,
                1,
            );
            let repetition = repetition_from_files(duration, &results);
            measured_results_by_run.push(results);
            Ok(repetition)
        },
    )?;
    let measured_results = measured_results_by_run.into_iter().flatten().collect();
    Ok(summary::file_profile(
        config,
        total_start,
        corpus,
        measured.total_duration(),
        measured_results,
        measured,
    ))
}

pub(super) struct PreparedCorpus {
    pub(super) linters: Vec<Arc<Linter>>,
    pub(super) prepared: Vec<PreparedFile>,
    pub(super) initial_errors: Vec<ProfileWorkloadSummary>,
    pub(super) manifest_digest: Option<String>,
    pub(super) setup_duration: Duration,
}

fn prepare_corpus(config: &ProfileConfig) -> Result<PreparedCorpus> {
    let setup_start = Instant::now();
    let (paths, manifest_digest, _) = support::selected_profile_paths(config)?;
    let linters = support::build_linters(config.provider, config.mode, &config.rules)?;
    let mut prepared = Vec::with_capacity(paths.len());
    let mut initial_errors = Vec::new();
    for path in &paths {
        match support::prepare_file(path) {
            Ok(file) => prepared.push(file),
            Err(error) => {
                let mut result = ProfileWorkloadSummary::new(path.clone());
                result.completion = ReportCompletion::Partial;
                result.error = Some(format!("{error:#}"));
                if !config.continue_on_error {
                    bail!(
                        "{}: {}",
                        result.path.display(),
                        result.error.as_deref().unwrap_or("file preparation failed")
                    );
                }
                initial_errors.push(result);
            }
        }
    }
    Ok(PreparedCorpus {
        linters,
        prepared,
        initial_errors,
        manifest_digest,
        setup_duration: setup_start.elapsed(),
    })
}
