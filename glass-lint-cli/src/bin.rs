//! Process entry point for the `glass-lint` executable.
//!
//! The library owns command execution so tests and other front ends can reuse
//! it. This module only translates its result into the CLI's three process
//! outcomes: success, findings that fail the configured policy, or an error.

use std::{io, process::ExitCode};

fn main() -> ExitCode {
    // Broken pipes are successful when a consumer intentionally stops reading
    // output (for example, `glass-lint rules | head`).
    match glass_lint_cli::run_from_env() {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(error) if is_broken_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn is_broken_pipe(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
}
