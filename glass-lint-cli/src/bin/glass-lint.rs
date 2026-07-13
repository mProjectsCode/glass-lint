use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glass_lint_core::{LintReport, Linter, RuleId, Severity};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    version,
    about = "Analyze JavaScript bundles and snippets with the Obsidian rule pack"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
    #[arg(long, value_enum, default_value_t = Provider::Obsidian, global = true)]
    provider: Provider,
}

#[derive(Clone, Copy, ValueEnum)]
enum Provider {
    Obsidian,
    Js,
}

#[derive(Subcommand)]
enum Command {
    Rules,
    Check {
        path: PathBuf,
        #[arg(long = "rule")]
        rules: Vec<String>,
        #[arg(long, value_enum, default_value_t = Profile::Recommended)]
        profile: Profile,
        #[arg(long, default_value_t = 10 * 1024 * 1024)]
        max_bytes: u64,
        #[arg(long, value_enum, default_value_t = Threshold::Error)]
        fail_on: Threshold,
    },
    Snippet {
        path: PathBuf,
        #[arg(long = "rule")]
        rules: Vec<String>,
        #[arg(long, value_enum, default_value_t = Profile::Recommended)]
        profile: Profile,
        #[arg(long, value_enum, default_value_t = Threshold::Error)]
        fail_on: Threshold,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Threshold {
    Info,
    Warning,
    Error,
    Never,
}

#[derive(Clone, Copy, ValueEnum)]
enum Profile {
    Recommended,
    Heuristic,
}

impl Profile {
    fn linter(self, provider: Provider) -> Linter {
        match self {
            Self::Recommended => match provider {
                Provider::Obsidian => glass_lint_obsidian::recommended_linter(),
                Provider::Js => glass_lint_js::recommended_linter(),
            },
            Self::Heuristic => match provider {
                Provider::Obsidian => glass_lint_obsidian::heuristic_linter(),
                Provider::Js => glass_lint_js::heuristic_linter(),
            },
        }
    }
}
impl Threshold {
    fn fails(self, severity: Severity) -> bool {
        match self {
            Self::Info => true,
            Self::Warning => severity >= Severity::Warning,
            Self::Error => severity >= Severity::Error,
            Self::Never => false,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(failed) => {
            if failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let args = Args::parse();
    let provider = args.provider;
    match args.command {
        Command::Rules => {
            println!(
                "{}",
                serde_json::to_string_pretty(&match provider {
                    Provider::Obsidian => glass_lint_obsidian::rule_catalog(),
                    Provider::Js => glass_lint_js::rule_catalog(),
                })?
            );
            Ok(false)
        }
        Command::Check {
            path,
            rules,
            profile,
            max_bytes,
            fail_on,
        } => analyze_paths(&path, &rules, profile, provider, max_bytes, fail_on),
        Command::Snippet {
            path,
            rules,
            profile,
            fail_on,
        } => analyze_paths(&path, &rules, profile, provider, u64::MAX, fail_on),
    }
}

fn analyze_paths(
    path: &Path,
    rules: &[String],
    profile: Profile,
    provider: Provider,
    max_bytes: u64,
    fail_on: Threshold,
) -> Result<bool> {
    let configured = profile.linter(provider);
    let linter = if rules.is_empty() {
        configured
    } else {
        let enabled = rules
            .iter()
            .map(|id| RuleId::parse(id.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?;
        Linter::with_rules(configured.catalog().clone(), enabled).map_err(anyhow::Error::msg)?
    };
    let mut paths = if path.is_dir() {
        WalkDir::new(path)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| {
                entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "js")
            })
            .map(walkdir::DirEntry::into_path)
            .collect()
    } else {
        vec![path.to_owned()]
    };
    paths.sort();
    if paths.is_empty() {
        bail!("no JavaScript files found at {}", path.display());
    }
    let mut reports: Vec<(String, LintReport)> = Vec::new();
    let mut failed = false;
    for path in paths {
        let metadata =
            fs::metadata(&path).with_context(|| format!("inspect {}", path.display()))?;
        if metadata.len() > max_bytes {
            bail!(
                "{} is {} bytes, exceeding --max-bytes {}",
                path.display(),
                metadata.len(),
                max_bytes
            );
        }
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let filename = path.to_string_lossy();
        let report = linter.lint(&source, &filename);
        failed |= !report.parse_diagnostics.is_empty()
            || report
                .findings
                .iter()
                .any(|finding| fail_on.fails(finding.severity));
        reports.push((filename.into_owned(), report));
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(failed)
}
