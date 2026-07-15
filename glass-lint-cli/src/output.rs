//! Deterministic report aggregation for stdout.

use std::io::{self, Write};

use anyhow::Result;
use console::Style;
use glass_lint_core::{
    LintReport, PrettyFile, PrettyOptions, PrettyReports, ProjectReport, RuleMetadata,
};
use serde::Serialize;

use crate::config::{Config, Output};

/// A linted file keeps its source so pretty rendering never rereads the file.
#[derive(Clone)]
pub struct FileOutput {
    pub(crate) path: String,
    pub(crate) report: LintReport,
    pub(crate) source: String,
}

#[derive(Clone, Copy, Serialize)]
pub struct Summary {
    pub(crate) files: usize,
    pub(crate) findings: usize,
    pub(crate) parse_diagnostics: usize,
}

#[derive(Clone, Copy, Serialize)]
pub struct ProjectSummary {
    pub(crate) files: usize,
    pub(crate) findings: usize,
    pub(crate) parse_diagnostics: usize,
    pub(crate) project_diagnostics: usize,
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

pub(crate) fn write_project_report(config: &Config, report: &ProjectReport) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_project_report_to(config, report, &mut stdout)?;
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
        .map(|file| glass_lint_core::ProjectFileReport {
            path: file.path.clone(),
            findings: file
                .report
                .findings
                .iter()
                .cloned()
                .map(|finding| glass_lint_core::ProjectFinding {
                    rule_id: finding.rule_id,
                    message_id: finding.message_id,
                    message: finding.message,
                    severity: finding.severity,
                    location: glass_lint_core::SourceLocation {
                        path: file.path.clone(),
                        range: finding.range,
                    },
                    evidence: finding
                        .evidence
                        .into_iter()
                        .map(|evidence| glass_lint_core::ProjectEvidence {
                            message: evidence.message,
                            location: evidence.range.map(|range| glass_lint_core::SourceLocation {
                                path: file.path.clone(),
                                range,
                            }),
                            source: evidence.source,
                        })
                        .collect(),
                })
                .collect(),
            parse_diagnostics: file.report.parse_diagnostics.clone(),
        })
        .collect::<Vec<_>>();
    let report = glass_lint_core::ProjectReport {
        schema_version: glass_lint_core::REPORT_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        operations: glass_lint_core::ProjectOperationCounts {
            files: summary.files,
            evidence: files
                .iter()
                .map(|file| {
                    file.findings
                        .iter()
                        .map(|finding| finding.evidence.len())
                        .sum::<usize>()
                })
                .sum(),
            ..glass_lint_core::ProjectOperationCounts::default()
        },
        files,
        diagnostics: Vec::new(),
    };
    serde_json::to_writer_pretty(&mut *out, &report)?;
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

fn write_project_report_to<W: Write>(
    config: &Config,
    report: &ProjectReport,
    out: &mut W,
) -> Result<()> {
    let summary = ProjectSummary {
        files: report.files.len(),
        findings: report.files.iter().map(|file| file.findings.len()).sum(),
        parse_diagnostics: report
            .files
            .iter()
            .map(|file| file.parse_diagnostics.len())
            .sum(),
        project_diagnostics: report.diagnostics.len(),
    };
    match config.cli.output {
        Output::Json => {
            serde_json::to_writer_pretty(&mut *out, report)?;
            writeln!(out)?;
        }
        Output::Pretty => write_project_pretty(config, report, summary, out)?,
    }
    Ok(())
}

fn write_project_pretty<W: Write>(
    config: &Config,
    report: &ProjectReport,
    summary: ProjectSummary,
    out: &mut W,
) -> Result<()> {
    let color = color_enabled(config);
    for file in &report.files {
        for finding in &file.findings {
            writeln!(
                out,
                "{}[{}] {}",
                Style::new()
                    .yellow()
                    .force_styling(color)
                    .apply_to(finding.severity),
                Style::new()
                    .cyan()
                    .force_styling(color)
                    .apply_to(finding.rule_id.to_string()),
                finding.message,
            )?;
            writeln!(
                out,
                "  {}:{}:{}",
                finding.location.path,
                finding.location.range.start.line,
                finding.location.range.start.column
            )?;
            for evidence in &finding.evidence {
                if let Some(location) = &evidence.location {
                    writeln!(
                        out,
                        "    {}:{}:{} - {}",
                        location.path,
                        location.range.start.line,
                        location.range.start.column,
                        evidence.message
                    )?;
                }
            }
        }
    }
    for file in &report.files {
        for diagnostic in &file.parse_diagnostics {
            writeln!(
                out,
                "diagnostic [parse] {} ({}:{}:{})",
                diagnostic.message,
                file.path,
                diagnostic
                    .range
                    .as_ref()
                    .map_or(0, |range| range.start.line),
                diagnostic
                    .range
                    .as_ref()
                    .map_or(0, |range| range.start.column)
            )?;
        }
    }
    for diagnostic in &report.diagnostics {
        if let Some(location) = &diagnostic.location {
            writeln!(
                out,
                "diagnostic [{}] {} ({}:{}:{})",
                diagnostic.code,
                diagnostic.message,
                location.path,
                location.range.start.line,
                location.range.start.column
            )?;
        } else {
            writeln!(
                out,
                "diagnostic [{}] {}",
                diagnostic.code, diagnostic.message
            )?;
        }
    }
    let summary_line = format!(
        "{} file(s), {} finding(s), {} parse diagnostic(s), {} project diagnostic(s)",
        summary.files, summary.findings, summary.parse_diagnostics, summary.project_diagnostics
    );
    let style = if summary.findings == 0
        && summary.parse_diagnostics == 0
        && summary.project_diagnostics == 0
    {
        Style::new().green()
    } else {
        Style::new().yellow()
    };
    writeln!(out, "{}", style.force_styling(color).apply_to(summary_line))?;
    writeln!(
        out,
        "operations: {} file(s), {} request(s), {} edge(s), {} export(s), {} effect projection(s), {} evidence item(s)",
        report.operations.files,
        report.operations.requests,
        report.operations.edges,
        report.operations.exports,
        report.operations.effect_projections,
        report.operations.evidence,
    )?;
    Ok(())
}

fn color_enabled(config: &Config) -> bool {
    config.cli.color && console::colors_enabled()
}
