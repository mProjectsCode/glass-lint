//! Profiling command adapter and stable human-readable summary output.

use std::num::NonZeroUsize;

use anyhow::Result;
use glass_lint_harness::{
    ProfileConfig, ProfileCorpusIdentity, ProfileSummary, ProfileWorkload, create_profile_manifest,
    run_profile,
};

use crate::args::ProfileArgs;

pub fn run(args: ProfileArgs) -> Result<bool> {
    if let Some(output) = &args.create_manifest {
        anyhow::ensure!(
            args.paths.len() == 1,
            "--create-manifest requires exactly one --path root"
        );
        let label = args.root_label.clone().unwrap_or_else(|| {
            args.paths[0].file_name().map_or_else(
                || "corpus".into(),
                |name| name.to_string_lossy().into_owned(),
            )
        });
        let manifest = create_profile_manifest(
            &args.paths[0],
            &args.include,
            &args.exclude,
            args.sample,
            args.seed,
            &label,
            output,
        )?;
        println!(
            "Manifest: {} file(s), {} byte(s), sha256 {}",
            manifest.file_count(),
            manifest.total_bytes(),
            manifest.digest()
        );
        return Ok(true);
    }
    // Translate CLI options once; the harness owns discovery, sampling, and
    // bounded parallel execution semantics.
    let repeat = NonZeroUsize::new(args.repeat)
        .ok_or_else(|| anyhow::anyhow!("--repeat must be at least 1"))?;
    let workers = NonZeroUsize::new(args.workers)
        .ok_or_else(|| anyhow::anyhow!("--workers must be at least 1"))?;
    let workload = if args.admitted_project {
        ProfileWorkload::AdmittedProject
    } else if args.project {
        ProfileWorkload::LoaderProject
    } else {
        ProfileWorkload::Files
    };
    let config = ProfileConfig::builder(args.paths)
        .include(args.include)
        .exclude(args.exclude)
        .sample(args.sample)
        .seed(args.seed)
        .warm_up(args.warm_up)
        .repeat(repeat)
        .continue_on_error(args.continue_on_error)
        .workers(workers)
        .provider(args.provider.into())
        .mode(args.profile.into())
        .rules(args.rules)
        .workload(workload)
        .manifest(args.manifest)
        .build()?;
    let report = run_profile(&config)?;
    print_report(&report, args.quiet);
    Ok(report.errors == 0)
}

fn print_report(report: &ProfileSummary, quiet: bool) {
    if !quiet {
        print_input_details(report);
    }
    print_aggregate_summary(report);
    if let ProfileCorpusIdentity::Verified(digest) = &report.workload.corpus {
        println!(
            "Manifest: sha256 {digest}, {} verified byte(s)",
            report.bytes
        );
    }
    print_phase_timings(report);
    print_slowest_inputs(report);
}

fn print_input_details(report: &ProfileSummary) {
    for input in &report.workload_results {
        match &input.error {
            Some(error) => eprintln!("input {}: {}", input.path.display(), error),
            None => eprintln!(
                "  {}: {:.1?} ({} finding(s), {} diagnostic(s))",
                input.path.display(),
                input.measured_elapsed,
                input.findings,
                input.diagnostics
            ),
        }
    }
}

fn print_aggregate_summary(report: &ProfileSummary) {
    println!(
        "Profile: {} input(s), {} byte(s), {} run(s), {} finding(s), {} parse/analysis diagnostic(s), {} error(s), setup {:.1?}, lint wall {:.1?}, total {:.1?}",
        report.inputs,
        report.bytes,
        report.runs,
        report.findings,
        report.diagnostics,
        report.errors,
        report.setup_duration,
        report.measured_elapsed,
        report.wall_duration
    );
    println!(
        "Median measured repetition: {:.1?}",
        report.median_repetition_duration
    );
    for (index, repetition) in report.repetitions.iter().enumerate() {
        println!(
            "Repetition {}: {:.1?}, {} finding(s), {} diagnostic(s), {:?}, runs {:?}, evidence {}, operations files={} requests={} edges={} exports={} scc_rounds={} effect_projections={} evidence={}",
            index + 1,
            repetition.duration,
            repetition.findings,
            repetition.diagnostics,
            repetition.completion,
            repetition.run_completions,
            repetition.evidence_order_digest,
            repetition.operation_counts.files,
            repetition.operation_counts.requests,
            repetition.operation_counts.edges,
            repetition.operation_counts.exports,
            repetition.operation_counts.scc_rounds,
            repetition.operation_counts.effect_projections,
            repetition.operation_counts.evidence,
        );
    }
}

fn print_phase_timings(report: &ProfileSummary) {
    println!(
        "Phases: discovery {:.1?}, reads {:.1?}, parse/local {:.1?}, resolution {:.1?}, linking/matching {:.1?}",
        report.phase_timings.discovery,
        report.phase_timings.reads,
        report.phase_timings.parse_and_local_analysis,
        report.phase_timings.resolution,
        report.phase_timings.linking_and_matching,
    );
    println!(
        "Operations: {} file(s), {} request(s), {} edge(s), {} export(s), {} SCC round(s), {} effect projection(s), {} evidence item(s)",
        report.operation_counts.files,
        report.operation_counts.requests,
        report.operation_counts.edges,
        report.operation_counts.exports,
        report.operation_counts.scc_rounds,
        report.operation_counts.effect_projections,
        report.operation_counts.evidence,
    );
}

fn print_slowest_inputs(report: &ProfileSummary) {
    let mut slowest: Vec<_> = report.workload_results.iter().collect();
    slowest.sort_by(|left, right| {
        right
            .measured_elapsed
            .cmp(&left.measured_elapsed)
            .then_with(|| left.path.cmp(&right.path))
    });
    if !slowest.is_empty() {
        println!("Slowest workload inputs:");
        for input in slowest.into_iter().take(10) {
            println!("  {:.1?} {}", input.measured_elapsed, input.path.display());
        }
    }
}
