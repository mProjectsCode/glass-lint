//! Clap-facing types for the small, stable command surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Command-line inputs that identify the invocation.
#[derive(Parser)]
#[command(version, about = "Analyze JavaScript or TypeScript files and bundles")]
pub struct Args {
    #[arg(long, conflicts_with = "config_json", global = true)]
    /// Optional TOML or JSON configuration file.
    pub config: Option<PathBuf>,
    #[arg(long, conflicts_with = "config", global = true)]
    /// Inline JSON configuration, mutually exclusive with `config`.
    pub config_json: Option<String>,
    /// The operation to perform after configuration is resolved.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level operation selected after configuration is loaded.
#[derive(Subcommand)]
pub enum Command {
    /// List the rules in the selected provider/profile catalog.
    Rules,
    /// Generate or verify the repository-wide rule reference.
    GenerateRules {
        /// Destination of the generated Markdown document.
        #[arg(short, long, default_value = "RULES.md")]
        output: PathBuf,
        /// Check that the destination is current without changing it.
        #[arg(long)]
        check: bool,
    },
    /// Analyze a project entry, directory, or `tsconfig.json`.
    Check {
        /// Entry file, directory, or `tsconfig.json` to load as a project.
        path: PathBuf,
    },
    /// Analyze a single source file without project linking.
    Snippet {
        /// One source file to lint without cross-file linking.
        path: PathBuf,
    },
}
