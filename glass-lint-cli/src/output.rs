//! Deterministic report aggregation for stdout.

use crate::config::{Config, Output};
use anyhow::Result;
use console::Style;
use glass_lint_core::{LintReport, PrettyFile, PrettyOptions, PrettyReports, RuleMetadata};
use serde::Serialize;
use std::io::{self, Write};

/// A linted file keeps its source so pretty rendering never rereads the file.
#[derive(Clone)]
pub struct FileOutput {
    pub(crate) path: String,
    pub(crate) report: LintReport,
    pub(crate) source: String,
}

#[derive(Serialize)]
struct JsonFileOutput<'a> {
    path: &'a str,
    report: &'a LintReport,
}

#[derive(Clone, Copy, Serialize)]
pub struct Summary {
    pub(crate) files: usize,
    pub(crate) findings: usize,
    pub(crate) parse_diagnostics: usize,
}

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: u32,
    files: Vec<JsonFileOutput<'a>>,
    summary: Summary,
}

pub fn write_rules(config: &Config) -> Result<bool> {
    let metadata = crate::config::catalog(config.cli.provider, config.cli.profile).metadata();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_rules_to(config, &metadata, &mut stdout)?;
    stdout.flush()?;
    Ok(false)
}

pub(crate) fn write_report(config: &Config, files: &[FileOutput], summary: Summary) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_report_to(config, files, summary, &mut stdout)?;
    stdout.flush().map_err(Into::into)
}

/// Kept separate from stdout acquisition so output bytes can be tested exactly.
fn write_rules_to<W: Write>(config: &Config, metadata: &[RuleMetadata], out: &mut W) -> Result<()> {
    let color = color_enabled(config);
    if matches!(config.cli.output, Output::Json) {
        serde_json::to_writer_pretty(&mut *out, metadata)?;
    } else {
        writeln!(
            out,
            "{}",
            Style::new()
                .bold()
                .cyan()
                .force_styling(color)
                .apply_to("ID\tSEVERITY\tDESCRIPTION")
        )?;
        for rule in metadata {
            writeln!(
                out,
                "{}\t{}\t{}",
                rule.id,
                severity_style(rule.default_severity)
                    .force_styling(color)
                    .apply_to(rule.default_severity),
                rule.description
            )?;
        }
    }
    writeln!(out)?;
    Ok(())
}

fn severity_style(severity: glass_lint_core::Severity) -> Style {
    match severity {
        glass_lint_core::Severity::Info => Style::new().blue(),
        glass_lint_core::Severity::Warning => Style::new().yellow(),
        glass_lint_core::Severity::Error => Style::new().red(),
    }
}

fn write_report_to<W: Write>(
    config: &Config,
    files: &[FileOutput],
    summary: Summary,
    out: &mut W,
) -> Result<()> {
    match config.cli.output {
        Output::Json => write_json(files, summary, out),
        Output::Pretty => write_pretty(config, files, summary, out),
    }
}

fn write_json<W: Write>(files: &[FileOutput], summary: Summary, out: &mut W) -> Result<()> {
    let files = files
        .iter()
        .map(|file| JsonFileOutput {
            path: &file.path,
            report: &file.report,
        })
        .collect();
    serde_json::to_writer_pretty(
        &mut *out,
        &Envelope {
            schema_version: 1,
            files,
            summary,
        },
    )?;
    writeln!(out)?;
    Ok(())
}

fn write_pretty<W: Write>(
    config: &Config,
    files: &[FileOutput],
    summary: Summary,
    out: &mut W,
) -> Result<()> {
    let options = PrettyOptions {
        max_width: config.cli.pretty_max_width,
        color: color_enabled(config),
    };
    let pretty_files = files
        .iter()
        .map(|file| PrettyFile::new(&file.report, &file.path, &file.source))
        .collect::<Vec<_>>();
    let rendered = PrettyReports::new(&pretty_files, options).to_string();
    if !rendered.is_empty() {
        write!(out, "{rendered}")?;
    }

    let summary_line = format!(
        "{} file(s), {} finding(s), {} parse diagnostic(s)",
        summary.files, summary.findings, summary.parse_diagnostics
    );
    let style = if summary.findings == 0 && summary.parse_diagnostics == 0 {
        Style::new().green()
    } else {
        Style::new().yellow()
    };
    writeln!(
        out,
        "{}",
        style
            .force_styling(color_enabled(config))
            .apply_to(summary_line)
    )?;
    Ok(())
}

fn color_enabled(config: &Config) -> bool {
    config.cli.color && console::colors_enabled()
}
