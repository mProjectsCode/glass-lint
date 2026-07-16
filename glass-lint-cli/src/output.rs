//! Deterministic report aggregation for stdout.

use std::{
    io::{self, Write},
    path::Path,
};

use anyhow::{Result, bail};
use console::{Style, measure_text_width};
use glass_lint_core::{
    LintReport, PrettyFile, PrettyOptions, PrettyReports, ProjectReport, RuleMetadata,
};
use serde::Serialize;

use crate::config::{Config, Output};

/// A linted file keeps its source so pretty rendering never rereads the file.
#[derive(Clone)]
pub struct FileOutput {
    /// Source is retained because pretty output must match the analyzed bytes.
    pub path: String,
    /// Findings and parse diagnostics produced for this source.
    pub report: LintReport,
    /// Original source text used for rendering snippets and locations.
    pub source: String,
}

#[derive(Clone, Copy, Serialize)]
pub struct Summary {
    /// Number of independently linted source files.
    pub files: usize,
    /// Number of rule findings across those files.
    pub findings: usize,
    /// Number of parse diagnostics across those files.
    pub parse_diagnostics: usize,
}

/// Counts for a linked project report, including diagnostics from resolution.
#[derive(Clone, Copy, Serialize)]
pub struct ProjectSummary {
    /// Number of files represented in the report.
    pub(crate) files: usize,
    /// Number of rule findings across those files.
    pub(crate) findings: usize,
    /// Number of parse diagnostics across those files.
    pub(crate) parse_diagnostics: usize,
    /// Number of project-level diagnostics such as unresolved links.
    pub project_diagnostics: usize,
}

/// Write the selected rule metadata and never request a failing exit status.
pub fn write_rules(config: &Config) -> Result<bool> {
    let metadata = crate::config::catalog(config.cli.provider, config.cli.profile).metadata();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_rules_to(config, &metadata, &mut stdout)?;
    stdout.flush()?;
    Ok(false)
}

/// Write reports for independently linted snippet files.
pub fn write_report(config: &Config, files: &[FileOutput], summary: Summary) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_report_to(config, files, summary, &mut stdout)?;
    stdout.flush().map_err(Into::into)
}

/// Write a report produced by resolver-aware project analysis.
pub fn write_project_report(config: &Config, report: &ProjectReport) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    write_project_report_to(config, report, &mut stdout)?;
    stdout.flush().map_err(Into::into)
}

/// Write the human-readable input mode before analysis begins.
pub fn write_mode(config: &Config, mode: &str, path: &Path) -> Result<()> {
    if matches!(config.cli.output, Output::Pretty) {
        let mut stdout = io::BufWriter::new(io::stdout().lock());
        writeln!(
            stdout,
            "mode: {} ({})",
            mode,
            visible_text(&path.display().to_string())
        )?;
        stdout.flush()?;
    }
    Ok(())
}

/// Kept separate from stdout acquisition so output bytes can be tested exactly.
fn write_rules_to<W: Write>(config: &Config, metadata: &[RuleMetadata], out: &mut W) -> Result<()> {
    let color = color_enabled(config);
    if matches!(config.cli.output, Output::Json) {
        serde_json::to_writer_pretty(&mut *out, metadata)?;
    } else {
        let mut table = Table::new([
            Style::new()
                .bold()
                .cyan()
                .force_styling(color)
                .apply_to("ID")
                .to_string(),
            Style::new()
                .bold()
                .cyan()
                .force_styling(color)
                .apply_to("SEVERITY")
                .to_string(),
            Style::new()
                .bold()
                .cyan()
                .force_styling(color)
                .apply_to("DESCRIPTION")
                .to_string(),
        ]);
        for rule in metadata {
            table.push([
                rule.id.to_string(),
                severity_style(rule.default_severity)
                    .force_styling(color)
                    .apply_to(rule.default_severity)
                    .to_string(),
                rule.description.clone(),
            ])?;
        }
        table.write(out)?;
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

/// Width-aware plain-text table used by human-readable CLI listings.
struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn new<I, S>(headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    fn push<I, S>(&mut self, row: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let row = row.into_iter().map(Into::into).collect::<Vec<_>>();
        if row.len() != self.headers.len() {
            bail!(
                "table row has {} columns; expected {}",
                row.len(),
                self.headers.len()
            );
        }
        self.rows.push(row);
        Ok(())
    }

    fn write<W: Write>(&self, out: &mut W) -> Result<()> {
        let mut widths = self
            .headers
            .iter()
            .map(|cell| measure_text_width(cell))
            .collect::<Vec<_>>();
        for row in &self.rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(measure_text_width(cell));
            }
        }

        Self::write_row(&self.headers, &widths, out)?;
        for row in &self.rows {
            Self::write_row(row, &widths, out)?;
        }
        Ok(())
    }

    fn write_row<W: Write>(row: &[String], widths: &[usize], out: &mut W) -> Result<()> {
        for (index, (cell, width)) in row.iter().zip(widths).enumerate() {
            if index > 0 {
                write!(out, "  ")?;
            }
            write!(out, "{cell}")?;
            if index + 1 < row.len() {
                write!(
                    out,
                    "{}",
                    " ".repeat(width.saturating_sub(measure_text_width(cell)))
                )?;
            }
        }
        writeln!(out)?;
        Ok(())
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

fn write_json<W: Write>(files: &[FileOutput], _summary: Summary, out: &mut W) -> Result<()> {
    // Reuse the public project report shape so snippet and project JSON remain
    // consumable by the same downstream tooling.
    let files = files
        .iter()
        .map(|file| {
            glass_lint_core::ProjectFileReport::from_lint_report(
                file.path.clone(),
                file.source.clone(),
                file.report.clone(),
            )
        })
        .collect::<Vec<_>>();
    let report =
        glass_lint_core::ProjectReport::from_file_reports(env!("CARGO_PKG_VERSION"), files);
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
        show_evidence_source: config.cli.show_evidence_source,
    };
    let pretty_files = files
        .iter()
        .map(|file| PrettyFile::new(&file.report, &file.path, &file.source))
        .collect::<Vec<_>>();
    write_pretty_files(&pretty_files, options, out)?;

    let summary_line = format!(
        "{} file(s), {} finding(s), {} parse diagnostic(s)",
        summary.files, summary.findings, summary.parse_diagnostics
    );
    write_summary(
        config,
        &summary_line,
        summary.findings == 0 && summary.parse_diagnostics == 0,
        out,
    )
}

fn write_pretty_files<W: Write>(
    pretty_files: &[PrettyFile<'_>],
    options: PrettyOptions,
    out: &mut W,
) -> Result<()> {
    let rendered = PrettyReports::new(pretty_files, options).to_string();
    if !rendered.is_empty() {
        write!(out, "{rendered}")?;
    }
    Ok(())
}

fn write_project_report_to<W: Write>(
    config: &Config,
    report: &ProjectReport,
    out: &mut W,
) -> Result<()> {
    let core_summary = report.summary();
    let summary = ProjectSummary {
        files: core_summary.files,
        findings: core_summary.findings,
        parse_diagnostics: core_summary.parse_diagnostics,
        project_diagnostics: core_summary.project_diagnostics,
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
    let files = report
        .files
        .iter()
        .map(|file| FileOutput {
            path: file.path.to_string(),
            report: file.to_lint_report(env!("CARGO_PKG_VERSION")),
            source: file.source.clone(),
        })
        .collect::<Vec<_>>();
    let options = PrettyOptions {
        max_width: config.cli.pretty_max_width,
        color: color_enabled(config),
        show_evidence_source: config.cli.show_evidence_source,
    };
    let pretty_files = files
        .iter()
        .map(|file| PrettyFile::new(&file.report, &file.path, &file.source))
        .collect::<Vec<_>>();
    write_pretty_files(&pretty_files, options, out)?;

    for diagnostic in &report.diagnostics {
        if let Some(location) = &diagnostic.location {
            writeln!(
                out,
                "diagnostic [{}] {} ({}:{}:{})",
                diagnostic.code,
                visible_text(&diagnostic.message),
                visible_text(location.path.as_str()),
                location.range.start.line,
                location.range.start.column
            )?;
        } else {
            writeln!(
                out,
                "diagnostic [{}] {}",
                diagnostic.code,
                visible_text(&diagnostic.message)
            )?;
        }
    }
    let summary_line = format!(
        "{} file(s), {} finding(s), {} parse diagnostic(s), {} project diagnostic(s), completion={:?}",
        summary.files,
        summary.findings,
        summary.parse_diagnostics,
        summary.project_diagnostics,
        report.completion
    );
    let clean =
        summary.findings == 0 && summary.parse_diagnostics == 0 && summary.project_diagnostics == 0;
    let summary_line = format!(
        "{summary_line}, operations: {} request(s), {} edge(s), {} export(s), {} effect projection(s), {} evidence item(s)",
        report.operations.requests,
        report.operations.edges,
        report.operations.exports,
        report.operations.effect_projections,
        report.operations.evidence,
    );
    write_summary(config, &summary_line, clean, out)?;
    Ok(())
}

fn write_summary<W: Write>(
    config: &Config,
    summary_line: &str,
    clean: bool,
    out: &mut W,
) -> Result<()> {
    let style = if clean {
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

/// Keep human output terminal-safe without changing the JSON contract.
fn visible_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            '\t' => "\\t".to_owned(),
            ch if ch.is_control() => format!("\\u{{{:04x}}}", ch as u32),
            ch => ch.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Table;

    #[test]
    fn table_aligns_columns_without_padding_the_last_column() {
        let mut table = Table::new(["ID", "SEVERITY", "DESCRIPTION"]);
        table.push(["x", "warning", "short"]).unwrap();

        let mut output = Vec::new();
        table.write(&mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "ID  SEVERITY  DESCRIPTION\nx   warning   short\n"
        );
    }
}
