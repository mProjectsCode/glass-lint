use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glass_lint_harness::{
    Adapter, ExternalAdapter, GlassLintAdapter, ProfileConfig, ProfileMode, ProfileProvider,
    comparison, failure_details, markdown, profile_folder, run_suite, summary,
};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
    Compare {
        path: PathBuf,
    },
    Profile {
        #[arg(long = "path", required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = ProfileProviderArg::Obsidian)]
        provider: ProfileProviderArg,
        #[arg(long, value_enum, default_value_t = ProfileModeArg::Recommended)]
        profile: ProfileModeArg,
        #[arg(long = "rule")]
        rules: Vec<String>,
        #[arg(long)]
        include: Vec<String>,
        #[arg(long)]
        exclude: Vec<String>,
        #[arg(long)]
        sample: Option<usize>,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long = "warm-up", default_value_t = 0)]
        warm_up: usize,
        #[arg(long, default_value_t = 1)]
        repeat: usize,
        #[arg(long, default_value_t = 1)]
        workers: usize,
        #[arg(long)]
        continue_on_error: bool,
        #[arg(long)]
        quiet: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Markdown,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileProviderArg {
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
enum ProfileModeArg {
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

struct ProgressLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ProgressLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Visitor(Option<String>);
        impl tracing::field::Visit for Visitor {
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "progress" {
                    self.0 = Some(value.to_string());
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut v = Visitor(None);
        event.record(&mut v);
        if let Some(line) = v.0 {
            eprintln!("{line}");
        }
    }
}
fn run() -> Result<bool> {
    let args = Args::parse();
    if let Command::Profile {
        paths,
        provider,
        profile,
        rules,
        include,
        exclude,
        sample,
        seed,
        warm_up,
        repeat,
        workers,
        continue_on_error,
        quiet,
    } = args.command
    {
        let report = profile_folder(&ProfileConfig {
            paths,
            include,
            exclude,
            sample,
            seed,
            warm_up,
            repeat,
            continue_on_error,
            workers,
            provider: provider.into(),
            mode: profile.into(),
            rules,
        })?;
        print_profile(&report, quiet);
        return Ok(report.errors > 0);
    }
    let mut adapters: Vec<Box<dyn Adapter>> = vec![Box::new(GlassLintAdapter)];
    for (name, command) in args.adapters {
        adapters.push(Box::new(ExternalAdapter { name, command }));
    }
    let (path, format, verify, compare_mode) = match args.command {
        Command::Verify { path } => (path, Format::Markdown, true, false),
        Command::Report { path, format } => (path, format, false, false),
        Command::Compare { path } => (path, Format::Markdown, false, true),
        Command::Profile { .. } => unreachable!("profile command was handled above"),
    };
    if compare_mode {
        eprintln!(
            "Running {} adapter(s) on cases in {}...",
            adapters.len(),
            path.display()
        );
        tracing_subscriber::registry()
            .with(ProgressLayer)
            .try_init()
            .ok();
    }
    let suite_start = Instant::now();
    let (report, case_timings) = run_suite(&path, &adapters)?;
    let suite_elapsed = suite_start.elapsed();

    if compare_mode {
        let mut tool_totals: BTreeMap<String, Duration> = BTreeMap::new();
        for timings in &case_timings {
            for (name, dur) in timings {
                *tool_totals.entry(name.clone()).or_default() += *dur;
            }
        }

        let tool_names: Vec<&str> = tool_totals.keys().map(String::as_str).collect();

        eprintln!(
            "Compared {} case(s) across tool(s): {}",
            report.cases.len(),
            tool_names.join(", ")
        );

        for (name, total) in &tool_totals {
            eprintln!("  {name}: {total:.1?}");
        }

        eprintln!("  total: {suite_elapsed:.1?}");

        let report_dir = PathBuf::from("reports");
        fs::create_dir_all(&report_dir)
            .with_context(|| format!("create {}", report_dir.display()))?;
        let report_path = report_dir.join("COMPARISON.md");
        let content = comparison(&report);
        fs::write(&report_path, &content)
            .with_context(|| format!("write {}", report_path.display()))?;
        eprintln!("Comparison report written to {}", report_path.display());
    } else if !verify {
        match format {
            Format::Markdown => print!("{}", markdown(&report)),
            Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        }
    } else {
        println!("{}", summary(&report));
    }

    if verify && !report.passed() {
        eprint!("{}", failure_details(&report));
    }

    if report.cases.is_empty() {
        bail!("no JavaScript harness cases found in {}", path.display());
    }

    Ok(compare_mode || report.passed())
}

fn print_profile(report: &glass_lint_harness::ProfileSummary, quiet: bool) {
    if !quiet {
        for file in &report.file_results {
            if let Some(error) = &file.error {
                eprintln!("error {}: {}", file.path.display(), error);
            } else {
                eprintln!(
                    "  {}: {:.1?} ({} finding(s), {} diagnostic(s))",
                    file.path.display(),
                    file.elapsed,
                    file.findings,
                    file.diagnostics
                );
            }
        }
    }
    println!(
        "Profile: {} file(s), {} byte(s), {} run(s), {} finding(s), {} parse/analysis diagnostic(s), {} error(s), setup {:.1?}, lint wall {:.1?}, total {:.1?}",
        report.files,
        report.bytes,
        report.runs,
        report.findings,
        report.diagnostics,
        report.errors,
        report.setup_elapsed,
        report.elapsed,
        report.total_elapsed
    );
    let mut slowest = report.file_results.iter().collect::<Vec<_>>();
    slowest.sort_by(|left, right| {
        right
            .elapsed
            .cmp(&left.elapsed)
            .then_with(|| left.path.cmp(&right.path))
    });
    if !slowest.is_empty() {
        println!("Slowest files:");
        for file in slowest.into_iter().take(10) {
            println!("  {:.1?} {}", file.elapsed, file.path.display());
        }
    }
}
