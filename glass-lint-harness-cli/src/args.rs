//! Clap-facing command and option definitions.

use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use glass_lint_harness::{ProfileCatalogProvider, RuleSelectionProfile};

/// Top-level CLI arguments shared by verification, reporting,
/// comparison, and profiling.
#[derive(Parser)]
#[command(version, about = "Run conformance cases and profiling workloads")]
pub struct Args {
    #[command(subcommand)]
    /// Operation to execute.
    pub command: Command,
    #[arg(long = "adapter", value_parser = parse_adapter, global = true)]
    /// External adapter registrations in `NAME=COMMAND` form.
    pub adapters: Vec<(String, PathBuf)>,
}

/// Commands for conformance cases, reports, comparison, and
/// profiling.
#[derive(Subcommand)]
pub enum Command {
    /// Run cases and return a failing exit status when expectations differ.
    Verify {
        /// Case file or directory to execute.
        path: PathBuf,
    },
    /// Render a report without treating mismatches as the primary output.
    Report {
        /// Case file or directory to execute.
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Markdown)]
        format: Format,
    },
    /// Run all configured adapters and write a comparison
    /// report.
    Compare {
        /// Case file or directory to execute.
        path: PathBuf,
    },
    /// Profile file inputs or project workloads using the configured provider
    /// and rule-selection profile.
    Profile(ProfileArgs),
}

/// Input-selection and execution controls for profiling.
///
/// `--project` selects loader-project work; `--admitted-project` selects the
/// explicit admitted project path; without either flag, inputs are profiled as
/// source files.
#[derive(ClapArgs)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProfileArgs {
    #[arg(long = "path", required = true)]
    pub paths: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = ProfileCatalogProviderArg::Obsidian)]
    pub provider: ProfileCatalogProviderArg,
    #[arg(long, value_enum, default_value_t = RuleSelectionProfileArg::Recommended)]
    pub profile: RuleSelectionProfileArg,
    #[arg(long = "rule")]
    pub rules: Vec<String>,
    #[arg(long)]
    pub include: Vec<String>,
    #[arg(long)]
    pub exclude: Vec<String>,
    #[arg(long)]
    pub sample: Option<usize>,
    #[arg(long, default_value_t = 0)]
    pub seed: u64,
    #[arg(long = "warm-up", default_value_t = 0)]
    pub warm_up: usize,
    #[arg(long, default_value_t = 1)]
    pub repeat: usize,
    #[arg(long, default_value_t = 1)]
    pub workers: usize,
    #[arg(long)]
    pub continue_on_error: bool,
    #[arg(long)]
    pub quiet: bool,
    #[arg(long, conflicts_with = "admitted_project")]
    pub project: bool,
    /// Exercise the explicit admitted-source project collection path.
    #[arg(long = "admitted-project", conflicts_with = "project")]
    pub admitted_project: bool,
    /// Verify and use an immutable corpus selection manifest.
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Create an immutable corpus selection manifest and exit.
    #[arg(long = "create-manifest", conflicts_with = "manifest")]
    pub create_manifest: Option<PathBuf>,
    /// Machine-independent label stored in a newly created manifest.
    #[arg(long = "root-label", requires = "create_manifest")]
    pub root_label: Option<String>,
}

/// Render format for the report command.
#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Markdown,
    Json,
}

/// Provider set whose rules are profiled.
#[derive(Clone, Copy, ValueEnum)]
pub enum ProfileCatalogProviderArg {
    Js,
    Obsidian,
    Both,
}

impl From<ProfileCatalogProviderArg> for ProfileCatalogProvider {
    fn from(provider: ProfileCatalogProviderArg) -> Self {
        match provider {
            ProfileCatalogProviderArg::Js => Self::Js,
            ProfileCatalogProviderArg::Obsidian => Self::Obsidian,
            ProfileCatalogProviderArg::Both => Self::Both,
        }
    }
}

/// Precision mode used by profiling.
#[derive(Clone, Copy, ValueEnum)]
pub enum RuleSelectionProfileArg {
    Recommended,
    Heuristic,
}

impl From<RuleSelectionProfileArg> for RuleSelectionProfile {
    fn from(mode: RuleSelectionProfileArg) -> Self {
        match mode {
            RuleSelectionProfileArg::Recommended => Self::Recommended,
            RuleSelectionProfileArg::Heuristic => Self::Heuristic,
        }
    }
}

fn parse_adapter(value: &str) -> Result<(String, PathBuf), String> {
    // Validate the separator here so malformed registrations fail during CLI
    // parsing rather than after case discovery has started.
    let (name, path) = value.split_once('=').ok_or("expected NAME=COMMAND")?;
    if name.is_empty() || path.is_empty() {
        return Err("expected NAME=COMMAND".into());
    }
    Ok((name.into(), path.into()))
}

#[cfg(test)]
mod tests;
