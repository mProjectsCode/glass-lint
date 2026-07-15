//! Clap-facing types for the small, stable command surface.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Command-line inputs that identify the invocation.
#[derive(Parser)]
#[command(version, about = "Analyze JavaScript or TypeScript files and bundles")]
pub struct Args {
    #[arg(long, conflicts_with = "config_json", global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, conflicts_with = "config", global = true)]
    pub config_json: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Rules,
    Check { path: PathBuf },
    Snippet { path: PathBuf },
}
