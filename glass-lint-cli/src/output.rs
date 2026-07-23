//! Deterministic report aggregation for stdout.

use std::{
    io::{self, Write},
    path::Path,
};

use anyhow::{Result, bail};
use console::{Style, measure_text_width};
use glass_lint_core::{
    PrettyFile, PrettyOptions, PrettyReports, RuleMetadata,
    project::{AnalysisReport, AnalysisReportSummary},
};

use crate::config::{Config, OutputFormat};

/// A linted file keeps its source so pretty rendering never rereads the file.
#[derive(Clone)]
pub struct FileOutput {
    /// Source is retained because pretty output must match the analyzed bytes.
    pub path: String,
    /// Complete one-file report, including completion and report diagnostics.
    pub report: AnalysisReport,
    /// Original source text used for rendering snippets and locations.
    pub source: String,
}

fn stdout_writer() -> io::BufWriter<io::StdoutLock<'static>> {
    io::BufWriter::new(io::stdout().lock())
}

/// Write the selected rule metadata and never request a failing exit status.
pub fn write_rules(config: &Config) -> Result<bool> {
    let metadata = crate::config::catalog(config.cli.provider, config.cli.profile).metadata();
    let mut stdout = stdout_writer();
    write_rules_to(config, &metadata, &mut stdout)?;
    stdout.flush()?;
    Ok(false)
}

/// Write reports for independently linted snippet files.
pub fn write_report(config: &Config, files: &[FileOutput]) -> Result<()> {
    let mut stdout = stdout_writer();
    write_report_to(config, files, &mut stdout)?;
    stdout.flush().map_err(Into::into)
}

/// Write a report produced by resolver-aware project analysis.
pub fn write_project_report(config: &Config, report: &AnalysisReport) -> Result<()> {
    let mut stdout = stdout_writer();
    write_project_report_to(config, report, &mut stdout)?;
    stdout.flush().map_err(Into::into)
}

/// Write the human-readable input mode before analysis begins.
pub fn write_mode(config: &Config, mode: &str, path: &Path) -> Result<()> {
    if matches!(config.cli.output, OutputFormat::Pretty) {
        let mut stdout = stdout_writer();
        writeln!(
            stdout,
            "mode: {} ({})",
            mode,
            glass_lint_core::visible_text(&path.display().to_string())
        )?;
        stdout.flush()?;
    }
    Ok(())
}

/// Kept separate from stdout acquisition so output bytes can be tested exactly.
fn write_rules_to<W: Write>(config: &Config, metadata: &[RuleMetadata], out: &mut W) -> Result<()> {
    let color = color_enabled(config);
    if matches!(config.cli.output, OutputFormat::Json) {
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
            table.push(Row::new([
                rule.id.to_string(),
                severity_style(rule.default_severity)
                    .force_styling(color)
                    .apply_to(rule.default_severity)
                    .to_string(),
                rule.description.clone(),
            ]))?;
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

struct Row(Vec<String>);

impl Row {
    fn new(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(values.into_iter().map(Into::into).collect())
    }
}

/// Width-aware plain-text table used by human-readable CLI listings.
struct Table {
    headers: Vec<String>,
    rows: Vec<Row>,
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

    fn push(&mut self, row: Row) -> Result<()> {
        if row.0.len() != self.headers.len() {
            bail!(
                "table row has {} columns; expected {}",
                row.0.len(),
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
            for (width, cell) in widths.iter_mut().zip(&row.0) {
                *width = (*width).max(measure_text_width(cell));
            }
        }

        Self::write_row(&self.headers, &widths, out)?;
        for row in &self.rows {
            Self::write_row(&row.0, &widths, out)?;
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

fn write_report_to<W: Write>(config: &Config, files: &[FileOutput], out: &mut W) -> Result<()> {
    let report = AnalysisReport::combine(files.iter().map(|file| file.report.clone()))?;
    let summary = report.summary();
    match config.cli.output {
        OutputFormat::Json => write_json(&report, out),
        OutputFormat::Pretty => write_pretty(config, files, summary, out),
    }
}

fn write_json<W: Write>(report: &AnalysisReport, out: &mut W) -> Result<()> {
    serde_json::to_writer_pretty(&mut *out, &report)?;
    writeln!(out)?;
    Ok(())
}

fn write_pretty<W: Write>(
    config: &Config,
    files: &[FileOutput],
    summary: AnalysisReportSummary,
    out: &mut W,
) -> Result<()> {
    let options = pretty_options(config);
    let pretty_files = files
        .iter()
        .flat_map(|file| {
            file.report
                .files
                .iter()
                .take(1)
                .map(|report| PrettyFile::new(report, &file.path, &file.source))
        })
        .collect::<Vec<_>>();
    write_pretty_files(&pretty_files, options, out)?;

    let summary_line = base_summary_line(summary);
    write_summary(config, &summary_line, summary_is_clean(summary), out)
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
    report: &AnalysisReport,
    out: &mut W,
) -> Result<()> {
    let summary = report.summary();
    match config.cli.output {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut *out, report)?;
            writeln!(out)?;
        }
        OutputFormat::Pretty => write_project_pretty(config, report, summary, out)?,
    }
    Ok(())
}

fn write_project_pretty<W: Write>(
    config: &Config,
    report: &AnalysisReport,
    summary: AnalysisReportSummary,
    out: &mut W,
) -> Result<()> {
    let options = pretty_options(config);
    let pretty_files = report
        .files
        .iter()
        .map(|file| PrettyFile::new(file, file.path.as_str(), ""))
        .collect::<Vec<_>>();
    write_pretty_files(&pretty_files, options, out)?;

    for diagnostic in &report.diagnostics {
        if let Some(location) = diagnostic.path().zip(diagnostic.range()) {
            writeln!(
                out,
                "diagnostic [{}] {} ({}:{}:{})",
                diagnostic.code(),
                glass_lint_core::visible_text(diagnostic.message()),
                glass_lint_core::visible_text(location.0.as_str()),
                location.1.start().line(),
                location.1.start().column()
            )?;
        } else {
            writeln!(
                out,
                "diagnostic [{}] {}",
                diagnostic.code(),
                glass_lint_core::visible_text(diagnostic.message())
            )?;
        }
    }
    let summary_line = format!(
        "{}, {} project diagnostic(s), completion={:?}",
        base_summary_line(summary),
        summary.report_diagnostics,
        report.completion
    );
    let clean = summary_is_clean(summary) && summary.report_diagnostics == 0;
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

fn pretty_options(config: &Config) -> PrettyOptions {
    PrettyOptions {
        max_width: config.cli.pretty_max_width,
        color: color_enabled(config),
        show_evidence_source: config.cli.show_evidence_source,
    }
}

fn base_summary_line(summary: AnalysisReportSummary) -> String {
    format!(
        "{} file(s), {} finding(s), {} parse diagnostic(s), {} analysis diagnostic(s)",
        summary.files, summary.findings, summary.parse_diagnostics, summary.file_diagnostics
    )
}

fn summary_is_clean(summary: AnalysisReportSummary) -> bool {
    summary.findings == 0 && summary.parse_diagnostics == 0 && summary.file_diagnostics == 0
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

#[cfg(test)]
mod tests {
    use glass_lint_core::{
        AnalysisLimits, Environment, Linter, Rule, RuleCatalog, Severity,
        project::{
            AnalysisDiagnostic, AnalysisReport, Diagnostic, DiagnosticCode, ReportCompletion,
        },
        rules::{Confidence, MatcherDecl},
    };

    use super::*;

    fn linter(semantic_operations: usize) -> Linter {
        let rule = Rule::builder("network.fetch")
            .description("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .declaration(
                MatcherDecl::builder()
                    .call_global("fetch")
                    .build()
                    .expect("valid matcher declaration"),
            )
            .build()
            .unwrap();
        let mut environment = Environment::default();
        environment.add_global("fetch").unwrap();
        Linter::new(
            glass_lint_core::LinterConfig::new(
                vec![RuleCatalog::new("test", vec![rule]).unwrap()],
                environment,
            )
            .with_limits(
                AnalysisLimits::default()
                    .with_semantic_operations(semantic_operations)
                    .unwrap(),
            ),
        )
        .unwrap()
    }

    fn output(path: &str, source: &str, report: AnalysisReport) -> FileOutput {
        FileOutput {
            path: path.into(),
            report,
            source: source.into(),
        }
    }

    fn json(files: &[FileOutput]) -> AnalysisReport {
        let mut bytes = Vec::new();
        let report = AnalysisReport::combine(files.iter().map(|file| file.report.clone())).unwrap();
        write_json(&report, &mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn table_aligns_columns_without_padding_the_last_column() {
        let mut table = Table::new(["ID", "SEVERITY", "DESCRIPTION"]);
        table.push(Row::new(["x", "warning", "short"])).unwrap();

        let mut output = Vec::new();
        table.write(&mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "ID  SEVERITY  DESCRIPTION\nx   warning   short\n"
        );
    }

    #[test]
    fn snippet_json_completion_matches_cli_exit_decision() {
        let source = "fetch('/remote');";
        let report = linter(1).lint_snippet(source, "partial.js").unwrap();
        let cli_failed = report.completion == ReportCompletion::Partial
            || !report.diagnostics.is_empty()
            || report.files.iter().any(|file| !file.diagnostics.is_empty());
        let combined = json(&[output("partial.js", source, report)]);

        assert!(cli_failed);
        assert_eq!(combined.completion, ReportCompletion::Partial);
        assert_eq!(
            combined.files[0].diagnostics[0].code(),
            "semantic_budget_exhausted"
        );
    }

    #[test]
    fn mixed_complete_parse_partial_and_semantic_partial_json_is_stable() {
        let complete_source = "fetch('/ok');";
        let broken_source = "fetch(";
        let semantic_source = "fetch('/partial');";
        let complete = linter(64).lint_snippet(complete_source, "a.js").unwrap();
        let parse_partial = linter(64).lint_snippet(broken_source, "b.js").unwrap();
        let mut semantic_partial = linter(1).lint_snippet(semantic_source, "c.js").unwrap();
        semantic_partial
            .diagnostics
            .push(Diagnostic::Project(AnalysisDiagnostic {
                code: DiagnosticCode::new("incomplete_project").unwrap(),
                message: "project scope retained".into(),
                location: None,
            }));
        let files = [
            output("c.js", semantic_source, semantic_partial),
            output("a.js", complete_source, complete),
            output("b.js", broken_source, parse_partial),
        ];

        let first = json(&files);
        let second = json(&files);
        assert_eq!(first, second);
        assert_eq!(first.completion, ReportCompletion::Partial);
        assert_eq!(
            first
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.js", "b.js", "c.js"]
        );
        assert_eq!(first.summary().parse_diagnostics, 1);
        assert_eq!(first.summary().file_diagnostics, 1);
        assert_eq!(first.diagnostics[0].code(), "incomplete_project");
    }
}
