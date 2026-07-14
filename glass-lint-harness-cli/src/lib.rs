//! Command-line orchestration for the harness.

pub mod args;

mod compare;
mod profile;

use anyhow::{Result, bail};
use glass_lint_harness::{Adapter, ExternalAdapter, GlassLintAdapter, run_suite};
use std::path::PathBuf;

/// Run a parsed harness command and return whether it passed.
pub fn run(args: args::Args) -> Result<bool> {
    init_telemetry();

    if let args::Command::Profile(profile) = args.command {
        return profile::run(profile);
    }

    let adapters = adapters(args.adapters);
    let (path, format, verify, compare) = match args.command {
        args::Command::Verify { path } => (path, args::Format::Markdown, true, false),
        args::Command::Report { path, format } => (path, format, false, false),
        args::Command::Compare { path } => (path, args::Format::Markdown, false, true),
        args::Command::Profile(_) => unreachable!("profile command was handled above"),
    };

    if compare {
        compare::begin(&path, adapters.len());
    }
    let suite_start = std::time::Instant::now();
    let (report, case_timings) = run_suite(&path, &adapters)?;

    if compare {
        compare::write_report(&report, &case_timings, suite_start.elapsed())?;
    } else if verify {
        println!("{}", glass_lint_harness::summary(&report));
        if !report.passed() {
            eprint!("{}", glass_lint_harness::failure_details(&report));
        }
    } else {
        match format {
            args::Format::Markdown => print!("{}", glass_lint_harness::markdown(&report)),
            args::Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        }
    }

    if report.cases.is_empty() {
        bail!("no JavaScript harness cases found in {}", path.display());
    }

    Ok(compare || report.passed())
}

fn adapters(configured: Vec<(String, PathBuf)>) -> Vec<Box<dyn Adapter>> {
    let mut adapters: Vec<Box<dyn Adapter>> = vec![Box::new(GlassLintAdapter)];
    adapters.extend(
        configured
            .into_iter()
            .map(|(name, command)| Box::new(ExternalAdapter { name, command }) as Box<dyn Adapter>),
    );
    adapters
}

fn init_telemetry() {
    let _ = glass_lint_core::telemetry::try_init_with_writer_and_color(
        glass_lint_core::telemetry::TelemetryLevel::Quiet,
        console::colors_enabled_stderr(),
        std::io::stderr,
    );
}
