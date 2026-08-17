use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use console::measure_text_width;
use glass_lint_core::project::FileReport;

/// Formatting options for terminal report rendering.
#[derive(Clone, Copy, Debug)]
pub struct PrettyOptions {
    /// Maximum rendered line width before evidence is abbreviated.
    pub max_width: usize,
    /// Whether severity and certainty labels use terminal colors.
    pub color: bool,
    /// Whether evidence includes source excerpts and carets.
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

/// Formatter for one file's findings and evidence.
pub struct PrettyReport<'a> {
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) options: PrettyOptions,
    pub(crate) line_starts: &'a [usize],
    pub(crate) line_cache: Arc<LineCache>,
}

/// A source-backed file report ready for grouped rendering.
#[derive(Clone)]
pub struct PrettyFile<'a> {
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) line_starts: Vec<usize>,
    pub(crate) line_cache: Arc<LineCache>,
}

impl<'a> PrettyFile<'a> {
    /// Associate a core file report with its filename and source text.
    pub fn new(report: &'a FileReport, filename: &'a str, source: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        let line_count = line_starts.len();
        Self {
            report,
            filename,
            source,
            line_starts,
            line_cache: Arc::new(LineCache::new(line_count)),
        }
    }
}

/// Formatter for findings grouped across multiple files.
pub struct PrettyReports<'a> {
    pub(crate) files: &'a [PrettyFile<'a>],
    pub(crate) options: PrettyOptions,
    pub(crate) file_index: HashMap<&'a str, &'a PrettyFile<'a>>,
}

impl<'a> PrettyReports<'a> {
    /// Create a grouped renderer for a set of files.
    pub fn new(files: &'a [PrettyFile<'a>], options: PrettyOptions) -> Self {
        let file_index = files.iter().map(|f| (f.filename, f)).collect();
        Self {
            files,
            options,
            file_index,
        }
    }
}

impl<'a> PrettyReport<'a> {
    /// Create a renderer for one file report.
    pub fn new(
        report: &'a FileReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
        line_starts: &'a [usize],
    ) -> Self {
        Self::with_cache(
            report,
            filename,
            source,
            options,
            line_starts,
            Arc::new(LineCache::new(line_starts.len())),
        )
    }

    pub(crate) fn new_with_cache(
        report: &'a FileReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
        line_starts: &'a [usize],
        line_cache: &Arc<LineCache>,
    ) -> Self {
        Self::with_cache(
            report,
            filename,
            source,
            options,
            line_starts,
            Arc::clone(line_cache),
        )
    }

    fn with_cache(
        report: &'a FileReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
        line_starts: &'a [usize],
        line_cache: Arc<LineCache>,
    ) -> Self {
        Self {
            report,
            filename,
            source,
            options,
            line_starts,
            line_cache,
        }
    }
}

/// Lazily computes display cells once per source line and shares them across
/// all evidence rows rendered for the same file.
pub struct LineCache {
    lines: Vec<OnceLock<Vec<Cell>>>,
}

impl LineCache {
    fn new(line_count: usize) -> Self {
        Self {
            lines: (0..line_count).map(|_| OnceLock::new()).collect(),
        }
    }

    pub(crate) fn get_or_init(&self, line_index: usize, line: &str) -> Option<&[Cell]> {
        self.lines.get(line_index).map(|cells| {
            cells
                .get_or_init(|| PrettyReport::cells_from_line(line))
                .as_slice()
        })
    }
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub(crate) ch: char,
    pub(crate) start: usize,
    pub(crate) width: usize,
}

impl Cell {
    pub(crate) fn write_display(&self, out: &mut impl std::fmt::Write) -> std::fmt::Result {
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

/// Return the terminal width of one character at a display column.
pub fn display_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        4 - (column % 4)
    } else {
        let mut buf = [0u8; 4];
        measure_text_width(ch.encode_utf8(&mut buf))
    }
}

/// Return the terminal width of a string.
pub fn display_width_str(text: &str) -> usize {
    measure_text_width(text)
}
