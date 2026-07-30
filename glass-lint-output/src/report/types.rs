use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
};

use console::measure_text_width;

use crate::project::FileReport;

#[derive(Clone, Copy, Debug)]
pub struct PrettyOptions {
    pub max_width: usize,
    pub color: bool,
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

pub struct PrettyReport<'a> {
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) options: PrettyOptions,
    pub(crate) line_starts: &'a [usize],
    pub(crate) line_cache: Option<&'a RefCell<BTreeMap<usize, Vec<Cell>>>>,
}

#[derive(Clone)]
pub struct PrettyFile<'a> {
    pub(crate) report: &'a FileReport,
    pub(crate) filename: &'a str,
    pub(crate) source: &'a str,
    pub(crate) line_starts: Vec<usize>,
    pub(crate) line_cache: RefCell<BTreeMap<usize, Vec<Cell>>>,
}

impl<'a> PrettyFile<'a> {
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

pub struct PrettyReports<'a> {
    pub(crate) files: &'a [PrettyFile<'a>],
    pub(crate) options: PrettyOptions,
    pub(crate) file_index: HashMap<&'a str, &'a PrettyFile<'a>>,
}

impl<'a> PrettyReports<'a> {
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
        measure_text_width(ch.encode_utf8(&mut buf))
    }
}

pub fn display_width_str(text: &str) -> usize {
    measure_text_width(text)
}
