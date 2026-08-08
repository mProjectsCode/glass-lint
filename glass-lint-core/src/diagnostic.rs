//! Provider-neutral diagnostic and serialized report data types.

use std::sync::OnceLock;

use glass_lint_datastructures::{ByteRange, InvalidSourceBoundary, Position, SourceRange};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{RuleId, project::SourceText};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
/// Severity exposed by the provider-neutral report schema.
pub enum Severity {
    /// Informational diagnostic.
    Info,
    /// Warning diagnostic.
    Warning,
    /// Error diagnostic.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Info => "info",
                Self::Warning => "warning",
                Self::Error => "error",
            }
        )
    }
}

impl Severity {
    /// Return the stable serialized spelling used by rule and report APIs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Precomputed byte-to-display-position boundaries for one source.
#[derive(Clone)]
pub struct SourceLineIndex {
    starts: Vec<usize>,
    source: SourceText,
    /// Fast path: when the source is pure ASCII, column offset is just the byte
    /// delta and no Unicode checkpoints are needed.
    is_ascii: bool,
    /// Per-line checkpoint intervals for fast column computation on long lines.
    /// Each checkpoint is `(byte_offset_from_line_start, char_count)`.
    /// Empty for lines under 256 bytes. Computed on first non-ASCII lookup.
    checkpoints: OnceLock<Vec<Vec<(usize, usize)>>>,
}

#[derive(Clone, Copy, Debug)]
struct ValidatedByteOffset(usize);

#[derive(Clone, Copy, Debug)]
struct ValidatedByteRange {
    start: ValidatedByteOffset,
    end: ValidatedByteOffset,
}

impl std::fmt::Debug for SourceLineIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceLineIndex")
            .field("line_count", &self.starts.len())
            .field("source_length", &self.source.len())
            .field("is_ascii", &self.is_ascii)
            .field("checkpoints_computed", &self.checkpoints.get().is_some())
            .finish()
    }
}

fn compute_checkpoints(source: &str, starts: &[usize]) -> Vec<Vec<(usize, usize)>> {
    starts
        .iter()
        .enumerate()
        .map(|(i, &line_start)| {
            let line_end = starts.get(i + 1).copied().unwrap_or(source.len());
            let line_len = line_end - line_start;
            if line_len < 256 {
                return Vec::new();
            }
            let line = &source[line_start..line_end];
            let mut checkpoints = Vec::new();
            checkpoints.push((0, 0));
            let mut next_marker = 256usize;
            for (char_count, (byte_offset, _)) in line.char_indices().enumerate() {
                if byte_offset >= next_marker {
                    checkpoints.push((byte_offset, char_count));
                    next_marker = next_marker.saturating_add(256);
                }
            }
            checkpoints
        })
        .collect()
}

impl SourceLineIndex {
    fn from_source(source: SourceText) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        let is_ascii = source.is_ascii();
        Self {
            starts,
            source,
            is_ascii,
            checkpoints: OnceLock::new(),
        }
    }

    /// Build an index once for a source before converting multiple ranges.
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self::from_source(source.into())
    }

    /// Build an index while retaining the source allocation admitted by the
    /// project boundary.
    #[must_use]
    pub fn from_text(source: SourceText) -> Self {
        Self::from_source(source)
    }

    /// Ensure per-line checkpoints are computed. Called only for non-ASCII
    /// sources on the first position lookup.
    fn ensure_checkpoints(&self) -> &Vec<Vec<(usize, usize)>> {
        self.checkpoints
            .get_or_init(|| compute_checkpoints(&self.source, &self.starts))
    }

    /// Convert a validated byte offset into a one-based display position.
    fn position(&self, offset: ValidatedByteOffset) -> Option<Position> {
        let offset = offset.0;
        let line = self
            .starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let line_start = self.starts[line];

        let column = if self.is_ascii {
            offset - line_start
        } else {
            let checkpoints = self.ensure_checkpoints();
            let line_checkpoints = &checkpoints[line];
            if line_checkpoints.is_empty() {
                self.source[line_start..offset].chars().count()
            } else {
                let byte_offset = offset - line_start;
                let checkpoint = line_checkpoints
                    .partition_point(|(bo, _)| *bo <= byte_offset)
                    .saturating_sub(1);
                let (check_byte, check_char) = line_checkpoints[checkpoint];
                check_char + self.source[line_start + check_byte..offset].chars().count()
            }
        };

        Position::new(
            u32::try_from(line).ok()?.checked_add(1)?,
            u32::try_from(column).ok()?.checked_add(1)?,
        )
        .ok()
    }

    fn range(&self, range: ValidatedByteRange) -> Option<SourceRange> {
        SourceRange::new(self.position(range.start)?, self.position(range.end)?).ok()
    }

    fn validate_range(
        &self,
        range: ByteRange,
    ) -> Result<ValidatedByteRange, InvalidSourceBoundary> {
        let start =
            usize::try_from(range.start()).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        let end = usize::try_from(range.end()).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        if end > self.source.len() {
            return Err(InvalidSourceBoundary::OutOfBounds);
        }
        if !self.source.is_char_boundary(start) || !self.source.is_char_boundary(end) {
            return Err(InvalidSourceBoundary::NotCharacterBoundary);
        }
        Ok(ValidatedByteRange {
            start: ValidatedByteOffset(start),
            end: ValidatedByteOffset(end),
        })
    }

    pub(crate) fn byte_range_from_offsets(
        &self,
        start: u32,
        end: u32,
    ) -> Result<ByteRange, InvalidSourceBoundary> {
        let range = ByteRange::new(start, end).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        self.validate_range(range)?;
        Ok(range)
    }

    pub(crate) fn range_from_offsets(
        &self,
        start: u32,
        end: u32,
    ) -> Result<SourceRange, InvalidSourceBoundary> {
        let range = ByteRange::new(start, end).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        self.range(self.validate_range(range)?)
            .ok_or(InvalidSourceBoundary::OutOfBounds)
    }

    /// Convert a checked byte range without clamping invalid parser output.
    ///
    /// ```
    /// use glass_lint_core::SourceLineIndex;
    /// use glass_lint_datastructures::ByteRange;
    ///
    /// let source = "éx";
    /// let index = SourceLineIndex::new(source);
    /// let range = index.try_range(ByteRange::new(0, 2).unwrap()).unwrap();
    /// assert_eq!(range.start().line(), 1);
    /// assert!(index.try_range(ByteRange::new(1, 2).unwrap()).is_err());
    /// ```
    pub fn try_range(&self, range: ByteRange) -> Result<SourceRange, InvalidSourceBoundary> {
        self.range(self.validate_range(range)?)
            .ok_or(InvalidSourceBoundary::OutOfBounds)
    }

    /// Borrow the source covered by a validated byte range.
    pub(crate) fn source_slice(&self, range: ByteRange) -> Option<&str> {
        let range = self.validate_range(range).ok()?;
        self.source.get(range.start.0..range.end.0)
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
/// Provider rule metadata exposed to front ends and integrations.
pub struct RuleMetadata {
    /// Stable namespaced rule identifier.
    id: RuleId,
    /// Provider-facing description of what the rule detects.
    description: String,
    /// Generated explanations of the validated query declarations.
    query_explanations: Vec<String>,
    /// Default severity assigned when the rule reports a finding.
    default_severity: Severity,
}

impl RuleMetadata {
    pub(crate) fn from_catalog(
        id: RuleId,
        description: String,
        query_explanations: Vec<String>,
        default_severity: Severity,
    ) -> Self {
        Self {
            id,
            description,
            query_explanations,
            default_severity,
        }
    }

    pub fn id(&self) -> &RuleId {
        &self.id
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn query_explanations(&self) -> &[String] {
        &self.query_explanations
    }

    pub fn default_severity(&self) -> Severity {
        self.default_severity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_converts_unicode_crlf_and_eof_positions() {
        let source = "é\r\nfetch();\n";
        let index = SourceLineIndex::new(source);
        let range = index.try_range(ByteRange::new(4, 9).unwrap()).unwrap();
        assert_eq!(range.start().line(), 2);
        assert_eq!(range.start().column(), 1);
        assert_eq!(range.end().line(), 2);
        assert_eq!(range.end().column(), 6);
        let eof = index.try_range(ByteRange::new(13, 13).unwrap()).unwrap();
        assert_eq!((eof.start().line(), eof.start().column()), (3, 1));
    }

    #[test]
    fn try_range_rejects_non_character_boundary_and_out_of_bounds() {
        let source = "aé\r\n🙂z";
        let index = SourceLineIndex::new(source);
        let range = index.try_range(ByteRange::new(5, 10).unwrap()).unwrap();
        assert_eq!((range.start().line(), range.start().column()), (2, 1));
        assert_eq!((range.end().line(), range.end().column()), (2, 3));
        assert_eq!(
            index.try_range(ByteRange::new(2, 3).unwrap()),
            Err(InvalidSourceBoundary::NotCharacterBoundary)
        );
        assert_eq!(
            index.try_range(ByteRange::new(0, 99).unwrap()),
            Err(InvalidSourceBoundary::OutOfBounds)
        );
    }

    #[test]
    fn line_index_handles_empty_and_eof_ranges() {
        let source = "last";
        let index = SourceLineIndex::new(source);
        let first = index.try_range(ByteRange::new(0, 1).unwrap()).unwrap();
        assert_eq!((first.start().line(), first.start().column()), (1, 1));
        let last = index.try_range(ByteRange::new(3, 4).unwrap()).unwrap();
        assert_eq!((last.end().line(), last.end().column()), (1, 5));
        let eof = index.try_range(ByteRange::new(4, 4).unwrap()).unwrap();
        assert_eq!((eof.start().line(), eof.start().column()), (1, 5));
        let empty = SourceLineIndex::new("");
        let range = empty.try_range(ByteRange::empty()).unwrap();
        assert_eq!((range.start().line(), range.start().column()), (1, 1));
    }

    #[test]
    fn invalid_parser_range_becomes_typed_error() {
        let source = "fetch();";
        let index = SourceLineIndex::new(source);
        assert_eq!(
            index.try_range(
                ByteRange::new(1, u32::try_from(source.len()).unwrap().saturating_add(1)).unwrap(),
            ),
            Err(InvalidSourceBoundary::OutOfBounds)
        );
    }

    #[test]
    fn new_and_from_text_delegate_to_same_constructor() {
        let source = "é\r\nfetch();\n";
        let index_borrowed = SourceLineIndex::new(source);
        let text: crate::project::SourceText = source.into();
        let index_owned = SourceLineIndex::from_text(text);

        // Both constructors produce identical positions.
        assert_eq!(
            index_borrowed.try_range(ByteRange::new(4, 5).unwrap()),
            index_owned.try_range(ByteRange::new(4, 5).unwrap()),
        );
        assert_eq!(
            index_borrowed.try_range(ByteRange::new(0, 2).unwrap()),
            index_owned.try_range(ByteRange::new(0, 2).unwrap()),
        );
        assert_eq!(
            index_borrowed.try_range(ByteRange::new(10, 11).unwrap()),
            index_owned.try_range(ByteRange::new(10, 11).unwrap()),
        );
    }
}
