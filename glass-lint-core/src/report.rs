//! Deterministic human-readable rendering of lint reports.
//!
//! Rendering is presentation-only: it groups already-owned findings by rule,
//! sorts evidence by file/location, and clips source excerpts to the display
//! width without changing the serialized report.

use std::{
    collections::BTreeMap,
    fmt::{self, Display},
};

use console::{Style, measure_text_width};

use crate::{LintReport, SourceRange};

#[derive(Clone, Copy, Debug)]
/// Display controls for pretty report rendering.
pub struct PrettyOptions {
    /// Maximum display width including the excerpt gutter.
    pub max_width: usize,
    /// Whether ANSI colors are enabled.
    pub color: bool,
}
impl Default for PrettyOptions {
    fn default() -> Self {
        Self {
            max_width: 160,
            color: false,
        }
    }
}

/// One report/source pair rendered as a file section.
pub struct PrettyReport<'a> {
    report: &'a LintReport,
    filename: &'a str,
    source: &'a str,
    options: PrettyOptions,
}

#[derive(Clone, Copy)]
/// Borrowed report/source input used by grouped rendering.
pub struct PrettyFile<'a> {
    report: &'a LintReport,
    filename: &'a str,
    source: &'a str,
}

impl<'a> PrettyFile<'a> {
    /// Pair a report with its authored filename and source text.
    pub fn new(report: &'a LintReport, filename: &'a str, source: &'a str) -> Self {
        Self {
            report,
            filename,
            source,
        }
    }
}

/// Multiple file reports rendered in deterministic order.
pub struct PrettyReports<'a> {
    files: &'a [PrettyFile<'a>],
    options: PrettyOptions,
}

impl<'a> PrettyReports<'a> {
    /// Construct a grouped renderer with display options.
    pub fn new(files: &'a [PrettyFile<'a>], options: PrettyOptions) -> Self {
        Self { files, options }
    }
}

impl<'a> PrettyReport<'a> {
    /// Construct a renderer for one report and source file.
    pub fn new(
        report: &'a LintReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
    ) -> Self {
        Self {
            report,
            filename,
            source,
            options,
        }
    }

    fn excerpt(
        &self,
        range: &SourceRange,
        indent: usize,
        out: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let line_no = range.start.line as usize;
        let Some(line) = self.source.split('\n').nth(line_no.saturating_sub(1)) else {
            return Ok(());
        };
        let line = line.trim_end_matches('\r');
        // The excerpt gutter is part of the configured display budget.
        let width = self.options.max_width.saturating_sub(indent).max(1);
        let gutter = " ".repeat(indent);
        let cells: Vec<Cell> = line
            .chars()
            .scan(0usize, |column, ch| {
                let width = display_width(ch, *column);
                let cell = Cell {
                    text: if ch == '\t' {
                        " ".repeat(width)
                    } else {
                        ch.to_string()
                    },
                    start: *column,
                    width,
                };
                *column += width;
                Some(cell)
            })
            .collect();

        let total_width = cells.last().map_or(0, |cell| cell.start + cell.width);
        let start = range.start.column.saturating_sub(1) as usize;
        let end = (range.end.column.saturating_sub(1) as usize).max(start + 1);
        let (window_start, window_end, leading, trailing) =
            select_window(&cells, total_width, start, width);

        let mut text = String::new();
        if leading {
            text.push_str("...");
        }
        for cell in &cells[window_start..window_end] {
            text.push_str(&cell.text);
        }
        if trailing {
            text.push_str("...");
        }
        writeln!(out, "{gutter}{text}")?;

        let origin = cells.get(window_start).map_or(0, |cell| cell.start);
        let caret_start = if leading { 3 } else { 0 } + start.saturating_sub(origin);
        let text_width = display_width_str(&text);
        let caret_len = end
            .saturating_sub(start)
            .max(1)
            .min(text_width.saturating_sub(caret_start).max(1));

        writeln!(
            out,
            "{gutter}{}{}",
            " ".repeat(caret_start.min(text_width)),
            Self::style(
                self.options.color,
                Style::new().yellow(),
                "^".repeat(caret_len)
            )
        )
    }
}

#[derive(Clone, Debug)]
struct Cell {
    text: String,
    start: usize,
    width: usize,
}

fn select_window(
    cells: &[Cell],
    total_width: usize,
    start: usize,
    width: usize,
) -> (usize, usize, bool, bool) {
    if total_width <= width {
        return (0, cells.len(), false, false);
    }
    let marker = 3;
    let content_width = width.saturating_sub(marker * 2).max(1);
    let center = cells
        .iter()
        .position(|cell| cell.start + cell.width > start)
        .unwrap_or(cells.len());
    let mut first = center.saturating_sub(content_width / 2);
    let mut last = first;
    let mut used = 0;
    while last < cells.len() && used + cells[last].width <= content_width {
        used += cells[last].width;
        last += 1;
    }
    while first > 0 && used + cells[first - 1].width <= content_width {
        first -= 1;
        used += cells[first].width;
    }
    while first < last && cells[first].start + cells[first].width <= start {
        first += 1;
    }
    if first == last && !cells.is_empty() {
        first = center.min(cells.len() - 1);
        last = first + 1;
    }
    (first, last, first > 0, last < cells.len())
}

fn display_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        4 - (column % 4)
    } else {
        measure_text_width(&ch.to_string())
    }
}

fn display_width_str(text: &str) -> usize {
    measure_text_width(text)
}

impl Display for PrettyReport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = PrettyFile::new(self.report, self.filename, self.source);
        write_files(&[file], self.options, f)
    }
}

impl Display for PrettyReports<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_files(self.files, self.options, f)
    }
}

fn write_files(
    files: &[PrettyFile<'_>],
    options: PrettyOptions,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let wrote_group = write_rule_groups(files, options, f)?;
    write_parse_diagnostics(files, options, wrote_group, f)
}

fn write_rule_groups(
    files: &[PrettyFile<'_>],
    options: PrettyOptions,
    f: &mut fmt::Formatter<'_>,
) -> Result<bool, fmt::Error> {
    let mut groups = BTreeMap::new();
    for file in files {
        for finding in &file.report.findings {
            let entries = groups.entry(&finding.rule_id).or_insert_with(Vec::new);
            if finding.evidence.is_empty() {
                entries.push((file, finding, None));
            } else {
                entries.extend(
                    finding
                        .evidence
                        .iter()
                        .map(|evidence| (file, finding, Some(evidence))),
                );
            }
        }
    }

    let mut wrote_group = false;
    for entries in groups.values_mut() {
        entries.sort_by(|left, right| {
            let left_range = left
                .2
                .and_then(|evidence| evidence.range.as_ref())
                .unwrap_or(&left.1.range);
            let right_range = right
                .2
                .and_then(|evidence| evidence.range.as_ref())
                .unwrap_or(&right.1.range);
            (
                left.0.filename,
                left_range.start.line,
                left_range.start.column,
                left_range.end.line,
                left_range.end.column,
            )
                .cmp(&(
                    right.0.filename,
                    right_range.start.line,
                    right_range.start.column,
                    right_range.end.line,
                    right_range.end.column,
                ))
        });
        if wrote_group {
            writeln!(f)?;
        }
        wrote_group = true;
        let finding = entries[0].1;
        writeln!(
            f,
            "{}[{}] {}",
            PrettyReport::style(
                options.color,
                match finding.severity {
                    crate::Severity::Info => Style::new().green(),
                    crate::Severity::Warning => Style::new().yellow(),
                    crate::Severity::Error => Style::new().red(),
                },
                finding.severity.to_string(),
            ),
            PrettyReport::style(
                options.color,
                Style::new().cyan(),
                finding.rule_id.to_string(),
            ),
            finding.message
        )?;

        for (file, finding, evidence) in entries {
            let range = evidence
                .and_then(|evidence| evidence.range.as_ref())
                .unwrap_or(&finding.range);

            let message = format!(
                "  {}:{}:{} - {}",
                file.filename,
                range.start.line,
                range.start.column,
                evidence.map_or_else(
                    || "match".to_string(),
                    |evidence| format!("evidence: {}", evidence.message),
                ),
            );

            writeln!(
                f,
                "{}",
                PrettyReport::style(options.color, Style::new().dim(), message)
            )?;
            PrettyReport::new(file.report, file.filename, file.source, options)
                .excerpt(range, 4, f)?;
        }
    }

    Ok(wrote_group)
}

fn write_parse_diagnostics(
    files: &[PrettyFile<'_>],
    options: PrettyOptions,
    leading_blank: bool,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let mut diagnostics = files
        .iter()
        .flat_map(|file| {
            file.report
                .parse_diagnostics
                .iter()
                .map(move |diagnostic| (file, diagnostic))
        })
        .collect::<Vec<_>>();
    diagnostics.sort_by_key(|(file, diagnostic)| {
        (
            file.filename,
            diagnostic.range.as_ref().map(|range| {
                (
                    range.start.line,
                    range.start.column,
                    range.end.line,
                    range.end.column,
                )
            }),
            diagnostic.code.as_str(),
        )
    });
    if !diagnostics.is_empty() {
        if leading_blank {
            writeln!(f)?;
        }
        writeln!(f, "parse diagnostics")?;
    }
    for (file, diagnostic) in diagnostics {
        if let Some(range) = &diagnostic.range {
            writeln!(
                f,
                "  {}:{}:{}: {}[{}]: {}",
                file.filename,
                range.start.line,
                range.start.column,
                PrettyReport::style(options.color, Style::new().red(), "parse"),
                diagnostic.code,
                diagnostic.message
            )?;
        } else {
            writeln!(
                f,
                "  {}: {}[{}]: {}",
                file.filename,
                PrettyReport::style(options.color, Style::new().red(), "parse"),
                diagnostic.code,
                diagnostic.message
            )?;
        }
    }

    Ok(())
}

impl PrettyReport<'_> {
    fn style<T: Display>(color: bool, style: Style, value: T) -> impl Display {
        style.force_styling(color).apply_to(value)
    }
}
