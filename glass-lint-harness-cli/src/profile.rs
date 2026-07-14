use anyhow::Result;
use glass_lint_harness::{ProfileConfig, ProfileSummary, profile_folder};

use crate::args::ProfileArgs;

pub(crate) fn run(args: ProfileArgs) -> Result<bool> {
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
    })?;
    print_report(&report, args.quiet);
    Ok(report.errors == 0)
}

fn print_report(report: &ProfileSummary, quiet: bool) {
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
