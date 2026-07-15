use std::{io, process::ExitCode};

use anyhow::Result;
use clap::Parser;
use glass_lint_cli::{
    args::{Args, Command},
    config, lint, output,
};

fn main() -> ExitCode {
    match run() {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(error) if is_broken_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let args = Args::parse();
    let config = config::load(&args)?;

    // The CLI owns subscriber installation; core only provides the formatter.
    let _ = glass_lint_core::telemetry::try_init_with_writer_and_color(
        config.cli.verbosity.telemetry(),
        config.cli.color && console::colors_enabled_stderr(),
        io::stderr,
    );
    tracing::debug!(target: "glass_lint::cli", source = "resolved", "configuration resolved");
    tracing::info!(
        target: "glass_lint::cli",
        command = ?std::mem::discriminant(&args.command),
        "command started"
    );

    if matches!(args.command, Command::Rules) {
        return output::write_rules(&config);
    }
    lint::run(&config, args.command)
}

fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
}
