//! Clap-facing command and option definitions.

use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use glass_lint_harness::{ProfileMode, ProfileProvider};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Run snippet conformance cases")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
    #[arg(long = "adapter", value_parser = parse_adapter, global = true)]
    pub adapters: Vec<(String, PathBuf)>,
}

#[derive(Subcommand)]
pub enum Command {
    Verify {
        path: PathBuf,
    },
    Report {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Markdown)]
        format: Format,
    },
    Compare {
        path: PathBuf,
    },
    Profile(ProfileArgs),
}

#[derive(ClapArgs)]
pub struct ProfileArgs {
    #[arg(long = "path", required = true)]
    pub paths: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = ProfileProviderArg::Obsidian)]
    pub provider: ProfileProviderArg,
    #[arg(long, value_enum, default_value_t = ProfileModeArg::Recommended)]
    pub profile: ProfileModeArg,
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
    #[arg(long)]
    pub project: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Markdown,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ProfileProviderArg {
    Js,
    Obsidian,
    Both,
}

impl From<ProfileProviderArg> for ProfileProvider {
    fn from(provider: ProfileProviderArg) -> Self {
        match provider {
            ProfileProviderArg::Js => Self::Js,
            ProfileProviderArg::Obsidian => Self::Obsidian,
            ProfileProviderArg::Both => Self::Both,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ProfileModeArg {
    Recommended,
    Heuristic,
}

impl From<ProfileModeArg> for ProfileMode {
    fn from(mode: ProfileModeArg) -> Self {
        match mode {
            ProfileModeArg::Recommended => Self::Recommended,
            ProfileModeArg::Heuristic => Self::Heuristic,
        }
    }
}

fn parse_adapter(value: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = value.split_once('=').ok_or("expected NAME=COMMAND")?;
    if name.is_empty() || path.is_empty() {
        return Err("expected NAME=COMMAND".into());
    }
    Ok((name.into(), path.into()))
}
