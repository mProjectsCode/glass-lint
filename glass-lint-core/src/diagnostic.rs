//! Provider-neutral diagnostic and serialized report data types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RuleId;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
/// A checked half-open byte range within one source artifact.
pub struct ByteRange {
    /// Zero-based byte offset of the first byte.
    start: u32,
    /// Zero-based byte offset immediately after the last byte.
    end: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReversedByteRange;

impl std::fmt::Display for ReversedByteRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("byte range start exceeds end")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidSourceBoundary {
    OutOfBounds,
    NotCharacterBoundary,
}

impl std::fmt::Display for InvalidSourceBoundary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::OutOfBounds => "byte range is outside the source",
            Self::NotCharacterBoundary => "byte range is not on UTF-8 character boundaries",
        })
    }
}

impl std::error::Error for InvalidSourceBoundary {}

impl<'de> Deserialize<'de> for ByteRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            start: u32,
            end: u32,
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::new(raw.start, raw.end).map_err(serde::de::Error::custom)
    }
}

impl ByteRange {
    /// Construct a range, rejecting reversed offsets.
    pub const fn new(start: u32, end: u32) -> Result<Self, ReversedByteRange> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(ReversedByteRange)
        }
    }

    /// Construct the empty range at the beginning of a source.
    #[must_use]
    pub const fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    /// Zero-based byte offset of the first byte in this range.
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Zero-based byte offset immediately after the last byte in this range.
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Number of bytes covered by this range.
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Return whether this range contains no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidPosition {
    ZeroLine,
    ZeroColumn,
}

impl std::fmt::Display for InvalidPosition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ZeroLine => "source position line must be one-based",
            Self::ZeroColumn => "source position column must be one-based",
        })
    }
}

impl std::error::Error for InvalidPosition {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReversedSourcePositionRange;

impl std::fmt::Display for ReversedSourcePositionRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("source range start exceeds end")
    }
}

impl std::error::Error for ReversedSourcePositionRange {}

/// One-based Unicode display position in a source file.
///
/// ```compile_fail
/// use glass_lint_core::Position;
/// let invalid = Position { line: 0, column: 1 };
/// ```
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Position {
    /// One-based source line.
    line: u32,
    /// One-based Unicode display column.
    column: u32,
}

impl Position {
    /// Construct a one-based display position, rejecting zero values.
    pub const fn new(line: u32, column: u32) -> Result<Self, InvalidPosition> {
        if line == 0 {
            Err(InvalidPosition::ZeroLine)
        } else if column == 0 {
            Err(InvalidPosition::ZeroColumn)
        } else {
            Ok(Self { line, column })
        }
    }

    /// One-based source line number.
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// One-based Unicode display column.
    pub const fn column(&self) -> u32 {
        self.column
    }
}

impl<'de> Deserialize<'de> for Position {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            line: u32,
            column: u32,
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::new(raw.line, raw.column).map_err(serde::de::Error::custom)
    }
}

/// Precomputed byte-to-display-position boundaries for one source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLineIndex {
    starts: Vec<usize>,
    source_len: usize,
}

impl SourceLineIndex {
    /// Build an index once for a source before converting multiple ranges.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
        Self {
            starts,
            source_len: source.len(),
        }
    }

    /// Convert a validated byte offset into a one-based display position.
    fn position(&self, source: &str, offset: usize) -> Position {
        let line = self
            .starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        Position::new(
            line.try_into().unwrap_or(u32::MAX).saturating_add(1),
            source[self.starts[line]..offset]
                .chars()
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        )
        .expect("line index always produces one-based positions")
    }

    /// Convert a byte start and length through this source's cached index.
    #[must_use]
    fn range(&self, source: &str, start: usize, length: usize) -> SourceRange {
        SourceRange::new(
            self.position(source, start),
            self.position(source, start.saturating_add(length)),
        )
        .expect("ordered byte offsets produce ordered source positions")
    }

    /// Convert a checked byte range without clamping invalid parser output.
    ///
    /// ```
    /// use glass_lint_core::{ByteRange, SourceLineIndex};
    ///
    /// let source = "éx";
    /// let index = SourceLineIndex::new(source);
    /// let range = index
    ///     .try_range(source, ByteRange::new(0, 2).unwrap())
    ///     .unwrap();
    /// assert_eq!(range.start().line(), 1);
    /// assert!(
    ///     index
    ///         .try_range(source, ByteRange::new(1, 2).unwrap())
    ///         .is_err()
    /// );
    /// ```
    pub fn try_range(
        &self,
        source: &str,
        range: ByteRange,
    ) -> Result<SourceRange, InvalidSourceBoundary> {
        let start = usize::try_from(range.start).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        let end = usize::try_from(range.end).map_err(|_| InvalidSourceBoundary::OutOfBounds)?;
        if source.len() != self.source_len || end > source.len() {
            return Err(InvalidSourceBoundary::OutOfBounds);
        }
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return Err(InvalidSourceBoundary::NotCharacterBoundary);
        }
        Ok(self.range(source, start, end - start))
    }
}

/// Inclusive-start, exclusive-end source range used by findings.
///
/// ```compile_fail
/// use glass_lint_core::{Position, SourceRange};
/// let position = Position::new(1, 1).unwrap();
/// let forged = SourceRange { start: position.clone(), end: position };
/// ```
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SourceRange {
    /// Inclusive start position.
    start: Position,
    /// Exclusive end position.
    end: Position,
}

impl SourceRange {
    /// Construct a source range, rejecting reversed start/end positions.
    pub fn new(start: Position, end: Position) -> Result<Self, ReversedSourcePositionRange> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(ReversedSourcePositionRange)
        }
    }

    /// Inclusive start position of this source range.
    pub const fn start(&self) -> &Position {
        &self.start
    }

    /// Exclusive end position of this source range.
    pub const fn end(&self) -> &Position {
        &self.end
    }

    /// Whether this range fully contains another range.
    pub fn contains(&self, inner: &Self) -> bool {
        self.start <= inner.start && inner.end <= self.end
    }
}

impl<'de> Deserialize<'de> for SourceRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            start: Position,
            end: Position,
        }
        let raw = Raw::deserialize(deserializer)?;
        Self::new(raw.start, raw.end).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// Provider rule metadata exposed to front ends and integrations.
pub struct RuleMetadata {
    /// Stable namespaced rule identifier.
    pub id: RuleId,
    /// Provider-facing description of what the rule detects.
    pub description: String,
    /// Default severity assigned when the rule reports a finding.
    pub default_severity: Severity,
    /// Stable message templates keyed by message ID. Each entry maps a
    /// message identifier (e.g. `"detected"`) to a human-readable template
    /// string used for report output.
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_ranges_reject_reversed_offsets_and_preserve_boundaries() {
        assert!(ByteRange::new(4, 3).is_err());
        assert_eq!(
            ByteRange::new(0, u32::MAX),
            Ok(ByteRange {
                start: 0,
                end: u32::MAX
            })
        );
        assert!(ByteRange::empty().is_empty());
    }

    #[test]
    fn line_index_converts_unicode_crlf_and_eof_positions() {
        let source = "é\r\nfetch();\n";
        let index = SourceLineIndex::new(source);
        let range = index.range(source, 4, 5);
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.column, 1);
        assert_eq!(range.end.line, 2);
        assert_eq!(range.end.column, 6);
        let eof = index.position(source, source.len());
        assert_eq!((eof.line, eof.column), (3, 1));
    }

    #[test]
    fn retained_ranges_reject_non_boundary_and_past_eof() {
        let source = "aé\r\n🙂z";
        let index = SourceLineIndex::new(source);
        let range = index
            .try_range(source, ByteRange::new(5, 10).unwrap())
            .unwrap();
        assert_eq!((range.start.line, range.start.column), (2, 1));
        assert_eq!((range.end.line, range.end.column), (2, 3));
        assert_eq!(
            index.try_range(source, ByteRange::new(2, 3).unwrap()),
            Err(InvalidSourceBoundary::NotCharacterBoundary)
        );
        assert_eq!(
            index.try_range(source, ByteRange::new(0, 99).unwrap()),
            Err(InvalidSourceBoundary::OutOfBounds)
        );
    }

    #[test]
    fn line_index_handles_empty_and_eof_ranges() {
        let source = "last";
        let index = SourceLineIndex::new(source);
        let first = index
            .try_range(source, ByteRange::new(0, 1).unwrap())
            .unwrap();
        assert_eq!((first.start.line, first.start.column), (1, 1));
        let last = index
            .try_range(source, ByteRange::new(3, 4).unwrap())
            .unwrap();
        assert_eq!((last.end.line, last.end.column), (1, 5));
        let eof = index
            .try_range(source, ByteRange::new(4, 4).unwrap())
            .unwrap();
        assert_eq!((eof.start.line, eof.start.column), (1, 5));
        let empty = SourceLineIndex::new("");
        let range = empty.try_range("", ByteRange::empty()).unwrap();
        assert_eq!((range.start.line, range.start.column), (1, 1));
    }

    #[test]
    fn invalid_parser_range_becomes_typed_error() {
        let source = "fetch();";
        let index = SourceLineIndex::new(source);
        assert_eq!(
            index.try_range(
                source,
                ByteRange::new(1, u32::try_from(source.len()).unwrap().saturating_add(1)).unwrap(),
            ),
            Err(InvalidSourceBoundary::OutOfBounds)
        );
    }
}
