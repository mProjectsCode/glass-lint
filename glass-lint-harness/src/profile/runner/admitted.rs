use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{
    Linter,
    project::{AnalysisReport, ReportCompletion},
};

use crate::profile::{
    config::{ProfileConfig, ProfileCorpusIdentity, ProfileWorkload, ProfileWorkloadIdentity},
    metrics::{accumulate_report, combined_digest, median_duration},
    runner::support,
    types::{
        MeasuredRepetitionAccumulator, PreparedFile, ProfileLinter, ProfileOperationCounts,
        ProfilePhaseTimings, ProfileRepetitionSummary, ProfileSummary,
    },
};

pub(super) fn run(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let root = fs::canonicalize(
        config
            .paths
            .first()
            .context("admitted-project requires one root")?,
    )?;
    let (paths, manifest_digest, verified_bytes) = support::selected_profile_paths(config)?;
    let prepared = paths
        .iter()
        .map(|path| support::prepare_file(path))
        .collect::<Result<Vec<_>>>()?;
    let linters = support::build_linters(config.provider, config.mode, &config.rules)?;
    let setup_duration = total_start.elapsed();
    let bytes = prepared.iter().map(|file| file.bytes).sum::<u64>();
    if let Some(verified_bytes) = verified_bytes
        && bytes != verified_bytes
    {
        bail!("verified manifest bytes changed during profile preparation");
    }
    let warm_run = || {
        for ProfileLinter(linter) in &linters {
            let _ = admitted_project_run(&root, &prepared, linter, config.workers.get())?;
        }
        Ok(())
    };
    let measured = MeasuredRepetitionAccumulator::measure(
        config.warm_up,
        config.repeat.get(),
        warm_run,
        || measure_repetition(&root, &prepared, &linters, config.workers.get()),
    )?;
    let findings = measured.repetitions.iter().map(|item| item.findings).sum();
    let diagnostics = measured
        .repetitions
        .iter()
        .map(|item| item.diagnostics)
        .sum();
    let operation_counts = support::sum_operation_counts(&measured.repetitions);
    let elapsed = measured.total_duration();
    let median_repetition_duration = median_duration(&measured.repetitions);
    let mut phase_timings = ProfilePhaseTimings::default();
    phase_timings.record_analyze_source(elapsed);
    phase_timings.record_total(total_start.elapsed());
    Ok(ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::AdmittedProject,
            corpus: manifest_digest.map_or(
                ProfileCorpusIdentity::Unverified,
                ProfileCorpusIdentity::Verified,
            ),
        },
        inputs: prepared.len(),
        bytes,
        findings,
        diagnostics,
        errors: 0,
        runs: config.repeat.get().saturating_mul(linters.len()),
        setup_duration,
        measured_elapsed: elapsed,
        wall_duration: total_start.elapsed(),
        repetitions: measured.repetitions,
        median_repetition_duration,
        workload_results: Vec::new(),
        phase_timings,
        operation_counts,
    })
}

fn measure_repetition(
    root: &Path,
    prepared: &[PreparedFile],
    linters: &[ProfileLinter],
    workers: usize,
) -> Result<ProfileRepetitionSummary> {
    let mut findings = 0;
    let mut diagnostics = 0;
    let mut operation_counts = ProfileOperationCounts::default();
    let mut completion = ReportCompletion::Complete;
    let mut run_completions = Vec::with_capacity(linters.len());
    let mut evidence_digests = Vec::new();
    for ProfileLinter(linter) in linters {
        let report = admitted_project_run(root, prepared, linter, workers)?;
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
    Ok(ProfileRepetitionSummary {
        duration: Duration::ZERO,
        findings,
        diagnostics,
        completion,
        run_completions,
        operation_counts,
        evidence_order_digest: combined_digest(&evidence_digests),
    })
}

fn admitted_project_run(
    root: &Path,
    prepared: &[PreparedFile],
    linter: &Linter,
    workers: usize,
) -> Result<AnalysisReport> {
    let mut session = linter.begin_project()?;
    let mut sources = Vec::with_capacity(prepared.len());
    for file in prepared {
        let relative = file.path.strip_prefix(root).with_context(|| {
            format!(
                "profile path outside admitted root: {}",
                file.path.display()
            )
        })?;
        sources.push(glass_lint_core::project::SourceFile::new(
            relative.to_string_lossy(),
            file.source.clone(),
        )?);
    }
    session.analyze_sources(
        sources,
        std::num::NonZeroUsize::new(workers).unwrap_or(std::num::NonZeroUsize::MIN),
    )?;
    Ok(session.finish_local().resolve([])?.finish()?)
}
