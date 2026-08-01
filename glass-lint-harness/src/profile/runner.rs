use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Barrier, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use glass_lint_core::{
    Linter, LinterConfig, RuleBaseline, RuleId, RuleOverride, RuleSelection, RuleState,
    project::{AnalysisReport, ReportCompletion, SourceFile},
};
use glass_lint_project::{ProjectLoader, ProjectSelection, ValidatedProjectLoadOptions};

use crate::{
    builtins::{self, BuiltinProfile},
    profile::{
        config::{
            ProfileCatalogProvider, ProfileConfig, ProfileCorpusIdentity, ProfileWorkload,
            ProfileWorkloadIdentity, RuleSelectionProfile, validate_config,
        },
        corpus::{discover_profile_files, sample_paths},
        metrics::{accumulate_report, combined_digest, median_duration, repetition_from_files},
        types::{
            MeasuredRepetitionAccumulator, PreparedFile, ProfileLinter, ProfileOperationCounts,
            ProfilePhaseTimings, ProfileProjectRun, ProfileProjectRunAccumulator,
            ProfileRepetitionSummary, ProfileSummary, ProfileSummaryAccumulator,
            ProfileWorkloadSummary, project_run_outcome, sum_operation_counts,
        },
    },
    profile_manifest::verify_profile_manifest,
};

pub fn run_profile(config: &ProfileConfig) -> Result<ProfileSummary> {
    validate_config(config)?;
    match config.workload {
        ProfileWorkload::LoaderProject => return profile_projects(config),
        ProfileWorkload::AdmittedProject => return profile_admitted_projects(config),
        ProfileWorkload::Files => {}
    }
    let total_start = Instant::now();

    let corpus = prepare_file_profile_corpus(config)?;

    let mut measured_results = Vec::new();
    let mut measured_results_by_run = Vec::new();
    let measured = MeasuredRepetitionAccumulator::measure(
        config.warm_up,
        config.repeat.get(),
        || {
            let _ = execute_file_profile(
                &corpus.prepared,
                &corpus.linters,
                config.workers.get(),
                1,
                0,
            );
            Ok(())
        },
        || {
            let (results, duration) = execute_file_profile(
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
    measured_results.extend(measured_results_by_run.into_iter().flatten());
    let lint_elapsed = measured.total_duration();

    Ok(file_profile_summary(
        config,
        total_start,
        corpus,
        lint_elapsed,
        measured_results,
        measured,
    ))
}

struct PreparedCorpus {
    linters: Vec<ProfileLinter>,
    prepared: Vec<PreparedFile>,
    initial_errors: Vec<ProfileWorkloadSummary>,
    manifest_digest: Option<String>,
    setup_duration: Duration,
}

fn prepare_file_profile_corpus(config: &ProfileConfig) -> Result<PreparedCorpus> {
    let setup_start = Instant::now();
    let (paths, manifest_digest, _) = selected_profile_paths(config)?;
    let linters = build_linters(config.provider, config.mode, &config.rules)?;

    let mut prepared = Vec::with_capacity(paths.len());
    let mut initial_errors = Vec::new();
    for path in &paths {
        match prepare_file(path) {
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

fn file_profile_summary(
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

fn profile_projects(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let (_, manifest_digest, _) = selected_profile_paths(config)?;
    let linters = build_linters(config.provider, config.mode, &config.rules)?;
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
    Ok(ProfileSummary {
        workload: ProfileWorkloadIdentity {
            mode: ProfileWorkload::LoaderProject,
            corpus: manifest_digest.map_or(
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
        setup_duration: phases.discovery() + phases.reads(),
        measured_elapsed: phases.parse_and_local_analysis()
            + phases.resolution()
            + phases.linking_and_matching(),
        wall_duration: phases.total(),
        median_repetition_duration: median_duration(&measured.repetitions),
        repetitions: measured.repetitions,
        workload_results: totals.workload_results,
        phase_timings: phases,
        operation_counts: counts,
    })
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
                    let (report, metrics, error) = profile_project_parts(outcome);
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

fn profile_admitted_projects(config: &ProfileConfig) -> Result<ProfileSummary> {
    let total_start = Instant::now();
    let root = fs::canonicalize(
        config
            .paths
            .first()
            .context("admitted-project requires one root")?,
    )?;
    let (paths, manifest_digest, verified_bytes) = selected_profile_paths(config)?;
    let prepared = paths
        .iter()
        .map(|path| prepare_file(path))
        .collect::<Result<Vec<_>>>()?;
    let linters = build_linters(config.provider, config.mode, &config.rules)?;
    let setup_duration = total_start.elapsed();
    let mut findings = 0;
    let mut diagnostics = 0;
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
        || measure_admitted_repetition(&root, &prepared, &linters, config.workers.get()),
    )?;
    for repetition in &measured.repetitions {
        findings += repetition.findings;
        diagnostics += repetition.diagnostics;
    }
    let operation_counts = sum_operation_counts(&measured.repetitions);
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

fn measure_admitted_repetition(
    root: &Path,
    prepared: &[PreparedFile],
    linters: &[ProfileLinter],
    workers: usize,
) -> Result<ProfileRepetitionSummary> {
    let mut repetition_findings = 0;
    let mut repetition_diagnostics = 0;
    let mut repetition_counts = ProfileOperationCounts::default();
    let mut repetition_completion = ReportCompletion::Complete;
    let mut run_completions = Vec::with_capacity(linters.len());
    let mut evidence_digests = Vec::new();
    for ProfileLinter(linter) in linters {
        let report = admitted_project_run(root, prepared, linter, workers)?;
        accumulate_report(
            &report,
            &mut repetition_findings,
            &mut repetition_diagnostics,
            &mut repetition_counts,
            &mut evidence_digests,
        );
        if report.completion() == ReportCompletion::Partial {
            repetition_completion = ReportCompletion::Partial;
        }
        run_completions.push(report.completion());
    }
    Ok(ProfileRepetitionSummary {
        duration: Duration::ZERO,
        findings: repetition_findings,
        diagnostics: repetition_diagnostics,
        completion: repetition_completion,
        run_completions,
        operation_counts: repetition_counts,
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

fn selected_profile_paths(
    config: &ProfileConfig,
) -> Result<(Vec<PathBuf>, Option<String>, Option<u64>)> {
    if let Some(manifest) = &config.manifest {
        let root = config
            .paths
            .first()
            .context("manifest profiling requires one root")?;
        let verified = verify_profile_manifest(root, manifest)?;
        return Ok((
            verified.paths,
            Some(verified.digest),
            Some(verified.total_bytes),
        ));
    }
    let mut paths = discover_profile_files(&config.paths, &config.include, &config.exclude)?;
    if let Some(sample) = config.sample {
        sample_paths(&mut paths, sample, config.seed);
    }
    Ok((paths, None, None))
}

fn prepare_file(path: &Path) -> Result<PreparedFile> {
    let metadata = fs::metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(PreparedFile {
        path: path.to_owned(),
        bytes: metadata.len(),
        source,
    })
}

fn build_linters(
    provider: ProfileCatalogProvider,
    mode: RuleSelectionProfile,
    rules: &[String],
) -> Result<Vec<ProfileLinter>> {
    let parsed = rules
        .iter()
        .map(|rule| RuleId::parse(rule.clone()).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    let providers = match provider {
        ProfileCatalogProvider::Js => vec!["js"],
        ProfileCatalogProvider::Obsidian => vec!["obsidian"],
        ProfileCatalogProvider::Both => vec!["js", "obsidian"],
    };
    let mut linters = Vec::new();
    for prefix in providers {
        let selected: Vec<_> = parsed
            .iter()
            .filter(|rule| rule.as_str().starts_with(&format!("{prefix}:")))
            .cloned()
            .collect();
        if !rules.is_empty() && selected.is_empty() {
            continue;
        }
        let provider = builtins::provider(prefix)?;
        let profile = match mode {
            RuleSelectionProfile::Recommended => BuiltinProfile::Recommended,
            RuleSelectionProfile::Heuristic => BuiltinProfile::Heuristic,
        };
        let linter = builtins::linter(provider, profile);
        let linter = if rules.is_empty() {
            linter
        } else {
            let selection = selected.into_iter().try_fold(
                RuleSelection::new(RuleBaseline::None),
                |selection, id| {
                    Ok::<_, glass_lint_core::LintConfigError>(
                        selection
                            .with_override(RuleOverride::new(id.to_string(), RuleState::Enabled)?),
                    )
                },
            )?;
            Linter::new(
                LinterConfig::new(
                    vec![linter.catalog().clone()],
                    linter.analysis_environment().clone(),
                )
                .with_rules(selection),
            )?
        };
        linters.push(ProfileLinter(Arc::new(linter)));
    }
    if linters.is_empty() {
        bail!("no selected rules belong to the chosen provider");
    }
    if parsed.iter().any(|rule| {
        !["js:", "obsidian:"]
            .iter()
            .any(|prefix| rule.as_str().starts_with(prefix))
    }) {
        bail!("rule does not belong to a supported profiling provider");
    }
    Ok(linters)
}

fn profile_file(
    file: &PreparedFile,
    linters: &[ProfileLinter],
    warm_up: usize,
    repeat: usize,
) -> ProfileWorkloadSummary {
    let mut findings = 0;
    let mut diagnostics = 0;
    let mut elapsed = Duration::ZERO;
    let mut completion = ReportCompletion::Complete;
    let mut run_completions = Vec::new();
    let mut operation_counts = ProfileOperationCounts::default();
    let mut evidence_digests = Vec::new();
    for iteration in 0..warm_up + repeat {
        for ProfileLinter(linter) in linters {
            let started = Instant::now();
            let filename = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("snippet.js");
            let report = linter
                .lint_source(
                    SourceFile::new(filename, file.source.clone())
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
                if report.completion() == ReportCompletion::Partial {
                    completion = ReportCompletion::Partial;
                }
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

fn execute_file_profile(
    prepared: &[PreparedFile],
    linters: &[ProfileLinter],
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

fn profile_project_parts(
    outcome: glass_lint_project::ProjectLoadOutcome,
) -> (
    AnalysisReport,
    glass_lint_project::ProjectLoadMetrics,
    Option<String>,
) {
    (
        outcome.report,
        outcome.metrics,
        outcome.partial_reason.map(|error| format!("{error:#}")),
    )
}
