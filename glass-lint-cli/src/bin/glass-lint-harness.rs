use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glass_lint_harness::{Adapter, ExternalAdapter, GlassLintAdapter, markdown, run_suite};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(version, about = "Run snippet conformance cases")]
struct Args {
    #[command(subcommand)]
    command: Command,
    #[arg(long = "adapter", value_parser = parse_adapter, global = true)]
    adapters: Vec<(String, PathBuf)>,
}
#[derive(Subcommand)]
enum Command {
    Verify {
        path: PathBuf,
    },
    Report {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Markdown)]
        format: Format,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Markdown,
    Json,
}

fn parse_adapter(value: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = value.split_once('=').ok_or("expected NAME=COMMAND")?;
    if name.is_empty() || path.is_empty() {
        return Err("expected NAME=COMMAND".into());
    }
    Ok((name.into(), path.into()))
}

fn main() -> ExitCode {
    match run() {
        Ok(passed) => {
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
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
    let mut adapters: Vec<Box<dyn Adapter>> = vec![Box::new(GlassLintAdapter)];
    for (name, command) in args.adapters {
        adapters.push(Box::new(ExternalAdapter { name, command }));
    }
    let (path, format, verify) = match args.command {
        Command::Verify { path } => (path, Format::Markdown, true),
        Command::Report { path, format } => (path, format, false),
    };
    let report = run_suite(&path, &adapters)?;
    if !verify {
        match format {
            Format::Markdown => print!("{}", markdown(&report)),
            Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        }
    }
    if verify && !report.passed() {
        for case in &report.cases {
            for (tool, result) in &case.tools {
                for error in &result.errors {
                    eprintln!("{} [{}]: {}", case.id, tool, error);
                }
            }
        }
    }
    if report.cases.is_empty() {
        bail!("no JavaScript harness cases found in {}", path.display());
    }
    Ok(report.passed())
}
