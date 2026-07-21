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

use crate::{FileReport, SourceRange};

#[derive(Clone, Copy, Debug)]
/// Display controls for pretty report rendering.
pub struct PrettyOptions {
    /// Maximum display width including the excerpt gutter.
    pub max_width: usize,
    /// Whether ANSI colors are enabled.
    pub color: bool,
    /// Whether evidence rows include source excerpts and carets.
    pub show_evidence_source: bool,
}
impl Default for PrettyOptions {
    fn default() -> Self {
        Self {
            max_width: 160,
            color: false,
            show_evidence_source: true,
        }
    }
}

/// One report/source pair rendered as a file section.
pub struct PrettyReport<'a> {
    report: &'a FileReport,
    filename: &'a str,
    source: &'a str,
    options: PrettyOptions,
    line_starts: &'a [usize],
}

#[derive(Clone)]
/// Borrowed report/source input used by grouped rendering.
pub struct PrettyFile<'a> {
    report: &'a FileReport,
    filename: &'a str,
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> PrettyFile<'a> {
    /// Pair a report with its authored filename and source text.
    pub fn new(report: &'a FileReport, filename: &'a str, source: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        Self {
            report,
            filename,
            source,
            line_starts,
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

    fn write_files(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wrote_group = self.write_rule_groups(f)?;
        self.write_parse_diagnostics(wrote_group, f)
    }

    fn write_rule_groups(&self, f: &mut fmt::Formatter<'_>) -> Result<bool, fmt::Error> {
        let mut groups = BTreeMap::new();
        for file in self.files {
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
                    .and_then(|evidence| evidence.location.as_ref())
                    .map_or(&left.1.location.range, |location| &location.range);
                let right_range = right
                    .2
                    .and_then(|evidence| evidence.location.as_ref())
                    .map_or(&right.1.location.range, |location| &location.range);
                (
                    left.0.filename,
                    left_range.start().line(),
                    left_range.start().column(),
                    left_range.end().line(),
                    left_range.end().column(),
                )
                    .cmp(&(
                        right.0.filename,
                        right_range.start().line(),
                        right_range.start().column(),
                        right_range.end().line(),
                        right_range.end().column(),
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
                    self.options.color,
                    match finding.severity {
                        crate::Severity::Info => Style::new().green(),
                        crate::Severity::Warning => Style::new().yellow(),
                        crate::Severity::Error => Style::new().red(),
                    },
                    finding.severity.to_string(),
                ),
                PrettyReport::style(
                    self.options.color,
                    Style::new().cyan(),
                    finding.rule_id.to_string(),
                ),
                visible_text(&finding.message)
            )?;

            for (file, finding, evidence) in entries {
                let range = evidence
                    .and_then(|evidence| evidence.location.as_ref())
                    .map_or(&finding.location.range, |location| &location.range);

                let message = format!(
                    "  {}:{}:{} - {}",
                    visible_text(file.filename),
                    range.start().line(),
                    range.start().column(),
                    evidence.map_or_else(
                        || "match".to_string(),
                        |evidence| format!("evidence: {}", visible_text(&evidence.message)),
                    ),
                );

                writeln!(
                    f,
                    "{}",
                    PrettyReport::style(self.options.color, Style::new().dim(), message)
                )?;
                if evidence.is_none() || self.options.show_evidence_source {
                    PrettyReport::new(
                        file.report,
                        file.filename,
                        file.source,
                        self.options,
                        &file.line_starts,
                    )
                    .excerpt(range, 4, f)?;
                }
            }
        }

        Ok(wrote_group)
    }

    fn write_parse_diagnostics(
        &self,
        leading_blank: bool,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let mut diagnostics = self
            .files
            .iter()
            .flat_map(|file| {
                file.report
                    .diagnostics
                    .iter()
                    .filter_map(move |diagnostic| {
                        let crate::Diagnostic::Parse { diagnostic, .. } = diagnostic else {
                            return None;
                        };
                        Some((file, diagnostic))
                    })
            })
            .collect::<Vec<_>>();
        diagnostics.sort_by_key(|(file, diagnostic)| {
            (
                file.filename,
                diagnostic.range.as_ref().map(|range| {
                    (
                        range.start().line(),
                        range.start().column(),
                        range.end().line(),
                        range.end().column(),
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
                    visible_text(file.filename),
                    range.start().line(),
                    range.start().column(),
                    PrettyReport::style(self.options.color, Style::new().red(), "parse"),
                    diagnostic.code,
                    visible_text(&diagnostic.message)
                )?;
            } else {
                writeln!(
                    f,
                    "  {}: {}[{}]: {}",
                    visible_text(file.filename),
                    PrettyReport::style(self.options.color, Style::new().red(), "parse"),
                    diagnostic.code,
                    visible_text(&diagnostic.message)
                )?;
            }
        }

        Ok(())
    }
}

impl<'a> PrettyReport<'a> {
    /// Select a visible window of cells for the excerpt display, returning
    /// the window range and whether leading/trailing ellipsis markers are
    /// needed.
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

    /// Construct a renderer for one report and source file.
    pub fn new(
        report: &'a FileReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
        line_starts: &'a [usize],
    ) -> Self {
        Self {
            report,
            filename,
            source,
            options,
            line_starts,
        }
    }

    fn excerpt(
        &self,
        range: &SourceRange,
        indent: usize,
        out: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let line_idx = range.start().line().saturating_sub(1) as usize;
        let Some(&line_start) = self.line_starts.get(line_idx) else {
            return Ok(());
        };
        let line_end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.source.len());
        let line = self.source[line_start..line_end].trim_end_matches(['\r', '\n']);
        // The excerpt gutter is part of the configured display budget.
        let width = self.options.max_width.saturating_sub(indent).max(1);
        let gutter = " ".repeat(indent);
        let cells: Vec<Cell> = line
            .chars()
            .scan(0usize, |column, ch| {
                let width = display_width(ch, *column);
                let cell = Cell {
                    ch,
                    start: *column,
                    width,
                };
                *column += width;
                Some(cell)
            })
            .collect();

        let total_width = cells.last().map_or(0, |cell| cell.start + cell.width);
        let start = range.start().column().saturating_sub(1) as usize;
        let end = (range.end().column().saturating_sub(1) as usize).max(start + 1);
        let (window_start, window_end, leading, trailing) =
            Self::select_window(&cells, total_width, start, width);

        let mut text = String::new();
        if leading {
            text.push_str("...");
        }
        for cell in &cells[window_start..window_end] {
            let _ = cell.write_display(&mut text);
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
    ch: char,
    start: usize,
    width: usize,
}

impl Cell {
    fn write_display(&self, out: &mut impl fmt::Write) -> fmt::Result {
        if self.ch == '\t' {
            for _ in 0..self.width {
                out.write_char(' ')?;
            }
        } else if self.ch.is_control() {
            write!(out, "\\u{{{:04x}}}", self.ch as u32)?;
        } else {
            out.write_char(self.ch)?;
        }
        Ok(())
    }
}

fn display_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        4 - (column % 4)
    } else {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        measure_text_width(s)
    }
}

fn display_width_str(text: &str) -> usize {
    measure_text_width(text)
}

/// Escape control characters before placing text in terminal-oriented output.
pub fn visible_text(value: &str) -> String {
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

impl Display for PrettyReport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = PrettyFile::new(self.report, self.filename, self.source);
        PrettyReports::new(&[file], self.options).write_files(f)
    }
}

impl Display for PrettyReports<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_files(f)
    }
}

impl PrettyReport<'_> {
    fn style<T: Display>(color: bool, style: Style, value: T) -> impl Display {
        style.force_styling(color).apply_to(value)
    }
}
