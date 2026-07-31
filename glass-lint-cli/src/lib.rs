//! Command-line orchestration for configuration, linting, and report output.
//!
//! The CLI deliberately delegates parsing and semantic analysis to the core
//! and project crates. It owns only user-facing command selection, exit
//! policy, and serialization of those results.

pub mod args;

mod config;
mod lint;
mod output;
mod rules_doc;
mod telemetry;

use std::io;

use anyhow::Result;
use clap::Parser;

/// Execute the command-line application from parsed arguments.
pub fn run(args: args::Args) -> Result<bool> {
    if let args::Command::GenerateRules { output, check } = &args.command {
        rules_doc::write_or_check(output, *check)?;
        return Ok(false);
    }

    // The boolean is deliberately separate from `Result`: operational errors
    // are exit code 2, while a valid report that crosses `fail_on` is exit 1.
    let config = config::load(&args)?;
    let _ = telemetry::try_init(
        telemetry::TelemetryOptions::new(config.cli.verbosity.telemetry())
            .color(config.cli.color && console::colors_enabled_stderr()),
        io::stderr,
    );
    tracing::debug!(target: "glass_lint::cli", source = "resolved", "configuration resolved");
    tracing::info!(
        target: "glass_lint::cli",
        command = ?std::mem::discriminant(&args.command),
        "command started"
    );
    if matches!(args.command, args::Command::Rules) {
        return output::write_rules(&config);
    }
    lint::run(&config, args.command)
}

/// Parse process arguments and execute the CLI.
pub fn run_from_env() -> Result<bool> {
    run(args::Args::parse())
}
