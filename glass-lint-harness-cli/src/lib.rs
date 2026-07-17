//! Command-line orchestration for the harness.
//!
//! The CLI owns presentation and exit-status policy; case discovery, adapters,
//! and analysis remain in `glass-lint-harness` so other front ends can reuse
//! them.

pub mod args;

mod compare;
mod profile;

use std::path::PathBuf;

use anyhow::{Result, bail};
use glass_lint_harness::{Adapter, ExternalAdapter, GlassLintAdapter, run_suite};

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
        println!("{}", glass_lint_harness::render_suite_summary(&report));
        if !report.passed() {
            eprint!("{}", glass_lint_harness::render_suite_failures(&report));
        }
    } else {
        match format {
            args::Format::Markdown => {
                print!("{}", glass_lint_harness::render_suite_markdown(&report));
            }
            args::Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        }
    }

    if report.cases.is_empty() {
        bail!("no JavaScript harness cases found in {}", path.display());
    }

    Ok(compare || report.passed())
}

fn adapters(configured: Vec<(String, PathBuf)>) -> Vec<Box<dyn Adapter>> {
    // Always include the in-process engine; external adapters extend the same
    // run rather than replacing the canonical implementation.
    let mut adapters: Vec<Box<dyn Adapter>> = vec![Box::new(GlassLintAdapter)];
    adapters.extend(
        configured
            .into_iter()
            .map(|(name, command)| Box::new(ExternalAdapter { name, command }) as Box<dyn Adapter>),
    );
    adapters
}

fn init_telemetry() {
    // CLI diagnostics belong on stderr and must not alter report stdout.
    let _ = glass_lint_core::telemetry::try_init(
        glass_lint_core::telemetry::TelemetryOptions::new(
            glass_lint_core::telemetry::TelemetryLevel::Quiet,
        )
        .color(console::colors_enabled_stderr()),
        std::io::stderr,
    );
}
