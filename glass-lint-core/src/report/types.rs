use std::{cell::RefCell, collections::BTreeMap};

use console::measure_text_width;

use crate::project::FileReport;

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
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) options: PrettyOptions,
    pub(crate) line_starts: &'a [usize],
    pub(crate) line_cache: Option<&'a RefCell<BTreeMap<usize, Vec<Cell>>>>,
}

#[derive(Clone)]
/// Borrowed report/source input used by grouped rendering.
pub struct PrettyFile<'a> {
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) line_starts: Vec<usize>,
    pub(crate) line_cache: RefCell<BTreeMap<usize, Vec<Cell>>>,
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
            line_cache: RefCell::new(BTreeMap::new()),
        }
    }
}

/// Multiple file reports rendered in deterministic order.
pub struct PrettyReports<'a> {
    pub(crate) files: &'a [PrettyFile<'a>],
    pub(crate) options: PrettyOptions,
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
            line_cache: None,
        }
    }

    pub(crate) fn new_with_cache(
        report: &'a FileReport,
        filename: &'a str,
        source: &'a str,
        options: PrettyOptions,
        line_starts: &'a [usize],
        line_cache: &'a RefCell<BTreeMap<usize, Vec<Cell>>>,
    ) -> Self {
        Self {
            report,
            filename,
            source,
            options,
            line_starts,
            line_cache: Some(line_cache),
        }
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

pub fn display_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        4 - (column % 4)
    } else {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        measure_text_width(s)
    }
}

pub fn display_width_str(text: &str) -> usize {
    measure_text_width(text)
}
