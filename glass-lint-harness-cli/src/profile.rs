//! Profiling command adapter and stable human-readable summary output.

use anyhow::Result;
use glass_lint_harness::{ProfileConfig, ProfileSummary, create_profile_manifest, profile_folder};

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
    let report = profile_folder(&ProfileConfig {
        paths: args.paths,
        include: args.include,
        exclude: args.exclude,
        sample: args.sample,
        seed: args.seed,
        warm_up: args.warm_up,
        repeat: args.repeat,
        continue_on_error: args.continue_on_error,
        workers: args.workers,
        provider: args.provider.into(),
        mode: args.profile.into(),
        rules: args.rules,
        project: args.project,
        admitted_project: args.admitted_project,
        manifest: args.manifest,
    })?;
    print_report(&report, args.quiet);
    Ok(report.errors == 0)
}

fn print_report(report: &ProfileSummary, quiet: bool) {
    // Keep per-file detail optional while always printing the aggregate summary
    // needed by scripts and interactive users.
    if !quiet {
        for file in &report.file_results {
            match &file.error {
                Some(error) => eprintln!("error {}: {}", file.path.display(), error),
                None => eprintln!(
                    "  {}: {:.1?} ({} finding(s), {} diagnostic(s))",
                    file.path.display(),
                    file.elapsed,
                    file.findings,
                    file.diagnostics
                ),
            }
        }
    }

    println!(
        "Profile: {} file(s), {} byte(s), {} run(s), {} finding(s), {} parse/analysis diagnostic(s), {} error(s), setup {:.1?}, lint wall {:.1?}, total {:.1?}",
        report.files,
        report.bytes,
        report.runs,
        report.findings,
        report.diagnostics,
        report.errors,
        report.setup_elapsed,
        report.elapsed,
        report.total_elapsed
    );
    println!("Median measured repetition: {:.1?}", report.median_elapsed);
    if report.workload.verified
        && let Some(digest) = &report.workload.corpus_digest
    {
        println!(
            "Manifest: sha256 {digest}, {} verified byte(s)",
            report.bytes
        );
    }
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

    let mut slowest = report.file_results.iter().collect::<Vec<_>>();
    slowest.sort_by(|left, right| {
        right
            .elapsed
            .cmp(&left.elapsed)
            .then_with(|| left.path.cmp(&right.path))
    });
    if !slowest.is_empty() {
        println!("Slowest files:");
        for file in slowest.into_iter().take(10) {
            println!("  {:.1?} {}", file.elapsed, file.path.display());
        }
    }
}
