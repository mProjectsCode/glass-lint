use std::process::ExitCode;

use clap::Parser;
use glass_lint_harness_cli::{args::Args, run};

/// Convert harness outcomes into the CLI's stable success/verification/error
/// codes.
fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}
