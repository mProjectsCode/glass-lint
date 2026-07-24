#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A half-open byte range `[start, end)` within a source file.
///
/// Invariant: `start <= end`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ByteRange {
    start: u32,
    end: u32,
}

/// Error returned when a [`ByteRange`] start exceeds its end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReversedByteRange;

impl std::fmt::Display for ReversedByteRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("byte range start exceeds end")
    }
}

/// Error returned when a [`ByteRange`] does not fit within a source or is not
/// aligned to UTF-8 character boundaries.
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

#[cfg(feature = "serde")]
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
    /// Creates a byte range `[start, end)`.
    ///
    /// Returns `Err(ReversedByteRange)` if `start > end`.
    pub const fn new(start: u32, end: u32) -> Result<Self, ReversedByteRange> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(ReversedByteRange)
        }
    }

    /// Zero-length range at position 0.
    #[must_use]
    pub const fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    /// Start offset (inclusive).
    pub const fn start(self) -> u32 {
        self.start
    }

    /// End offset (exclusive).
    pub const fn end(self) -> u32 {
        self.end
    }

    /// Length in bytes.
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether this is a zero-length range.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Error returned when a [`Position`] has a zero line or column.
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

/// Error returned when a [`SourceRange`] start exceeds its end.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReversedSourcePositionRange;

impl std::fmt::Display for ReversedSourcePositionRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("source range start exceeds end")
    }
}

impl std::error::Error for ReversedSourcePositionRange {}

/// A one-based line/column position in a source file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Position {
    line: u32,
    column: u32,
}

impl Position {
    /// Creates a new position.
    ///
    /// Returns `Err(InvalidPosition)` if `line == 0` or `column == 0`.
    pub const fn new(line: u32, column: u32) -> Result<Self, InvalidPosition> {
        if line == 0 {
            Err(InvalidPosition::ZeroLine)
        } else if column == 0 {
            Err(InvalidPosition::ZeroColumn)
        } else {
            Ok(Self { line, column })
        }
    }

    /// One-based line number.
    pub const fn line(&self) -> u32 {
        self.line
    }

    /// One-based column number.
    pub const fn column(&self) -> u32 {
        self.column
    }
}

#[cfg(feature = "serde")]
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

/// A half-open source range `[start, end)` identified by line/column
/// positions.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SourceRange {
    start: Position,
    end: Position,
}

impl SourceRange {
    /// Creates a source range.
    ///
    /// Returns `Err(ReversedSourcePositionRange)` if `start > end`.
    pub fn new(start: Position, end: Position) -> Result<Self, ReversedSourcePositionRange> {
        if start <= end {
            Ok(Self { start, end })
        } else {
            Err(ReversedSourcePositionRange)
        }
    }

    /// Start position (inclusive).
    pub const fn start(&self) -> &Position {
        &self.start
    }

    /// End position (exclusive).
    pub const fn end(&self) -> &Position {
        &self.end
    }

    /// Returns `true` if `inner` is wholly contained within `self`.
    pub fn contains(&self, inner: &Self) -> bool {
        self.start <= inner.start && inner.end <= self.end
    }
}

#[cfg(feature = "serde")]
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
    fn byte_range_len() {
        let r = ByteRange::new(3, 7).unwrap();
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn byte_range_empty_is_empty() {
        let r = ByteRange::new(5, 5).unwrap();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn position_rejects_zero_line() {
        assert_eq!(Position::new(0, 1), Err(InvalidPosition::ZeroLine));
    }

    #[test]
    fn position_rejects_zero_column() {
        assert_eq!(Position::new(1, 0), Err(InvalidPosition::ZeroColumn));
    }

    #[test]
    fn position_valid() {
        let p = Position::new(1, 1).unwrap();
        assert_eq!(p.line(), 1);
        assert_eq!(p.column(), 1);
    }

    #[test]
    fn position_display_for_invalid() {
        let e = InvalidPosition::ZeroLine;
        assert_eq!(e.to_string(), "source position line must be one-based");
        let e = InvalidPosition::ZeroColumn;
        assert_eq!(e.to_string(), "source position column must be one-based");
    }

    #[test]
    fn source_range_rejects_reversed() {
        let a = Position::new(2, 1).unwrap();
        let b = Position::new(1, 1).unwrap();
        assert!(SourceRange::new(a, b).is_err());
    }

    #[test]
    fn source_range_valid() {
        let s = Position::new(1, 1).unwrap();
        let e = Position::new(1, 5).unwrap();
        let r = SourceRange::new(s, e).unwrap();
        assert_eq!(r.start(), &Position::new(1, 1).unwrap());
        assert_eq!(r.end(), &Position::new(1, 5).unwrap());
    }

    #[test]
    fn source_range_contains() {
        let outer =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        let inner =
            SourceRange::new(Position::new(1, 5).unwrap(), Position::new(2, 10).unwrap()).unwrap();
        assert!(outer.contains(&inner));
    }

    #[test]
    fn source_range_does_not_contain_wider_range() {
        let outer =
            SourceRange::new(Position::new(2, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        let wider =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        assert!(!outer.contains(&wider));
    }

    #[test]
    fn invalid_source_boundary_display() {
        assert_eq!(
            InvalidSourceBoundary::OutOfBounds.to_string(),
            "byte range is outside the source"
        );
        assert_eq!(
            InvalidSourceBoundary::NotCharacterBoundary.to_string(),
            "byte range is not on UTF-8 character boundaries"
        );
    }

    #[test]
    fn reversed_byte_range_display() {
        assert_eq!(
            ReversedByteRange.to_string(),
            "byte range start exceeds end"
        );
    }

    #[test]
    fn byte_range_start_end_accessors() {
        let r = ByteRange::new(3, 7).unwrap();
        assert_eq!(r.start(), 3);
        assert_eq!(r.end(), 7);
    }

    #[test]
    fn byte_range_default_is_empty() {
        let r = ByteRange::default();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn byte_range_empty_is_not_none() {
        assert!(ByteRange::empty().is_empty());
    }

    #[test]
    fn byte_range_non_empty_is_not_empty() {
        let r = ByteRange::new(1, 2).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn byte_range_max_len() {
        let r = ByteRange::new(0, u32::MAX).unwrap();
        assert_eq!(r.len(), u32::MAX);
    }

    #[test]
    fn byte_range_hash_and_ord() {
        use std::collections::HashSet;
        let a = ByteRange::new(1, 3).unwrap();
        let b = ByteRange::new(1, 3).unwrap();
        let c = ByteRange::new(1, 4).unwrap();
        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        assert_eq!(set.len(), 1);
        set.insert(c);
        assert_eq!(set.len(), 2);
        assert!(a < c);
    }

    #[test]
    fn position_start_end_values() {
        let p = Position::new(2, 3).unwrap();
        assert_eq!(p.line(), 2);
        assert_eq!(p.column(), 3);
    }

    #[test]
    fn position_max_values() {
        let p = Position::new(u32::MAX, u32::MAX).unwrap();
        assert_eq!(p.line(), u32::MAX);
        assert_eq!(p.column(), u32::MAX);
    }

    #[test]
    fn position_ordering() {
        let a = Position::new(1, 5).unwrap();
        let b = Position::new(2, 1).unwrap();
        let c = Position::new(2, 1).unwrap();
        assert!(a < b);
        assert_eq!(b, c);
    }

    #[test]
    fn position_error_is_error() {
        use std::error::Error;
        let e = InvalidPosition::ZeroLine;
        assert!(e.source().is_none());
    }

    #[test]
    fn source_range_contains_self() {
        let start = Position::new(1, 1).unwrap();
        let end = Position::new(3, 1).unwrap();
        let r = SourceRange::new(start, end).unwrap();
        assert!(r.contains(&r));
    }

    #[test]
    fn source_range_contains_start_boundary() {
        let outer =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        let same_start =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(2, 1).unwrap()).unwrap();
        assert!(outer.contains(&same_start));
    }

    #[test]
    fn source_range_contains_end_boundary() {
        let outer =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        let same_end =
            SourceRange::new(Position::new(2, 1).unwrap(), Position::new(3, 1).unwrap()).unwrap();
        assert!(outer.contains(&same_end));
    }

    #[test]
    fn source_range_does_not_contain_disjoint() {
        let outer =
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(2, 1).unwrap()).unwrap();
        let disjoint =
            SourceRange::new(Position::new(3, 1).unwrap(), Position::new(4, 1).unwrap()).unwrap();
        assert!(!outer.contains(&disjoint));
    }

    #[test]
    fn reversed_source_position_range_display() {
        let err = ReversedSourcePositionRange;
        assert_eq!(err.to_string(), "source range start exceeds end");
    }

    #[test]
    fn reversed_source_position_range_is_error() {
        use std::error::Error;
        let err = ReversedSourcePositionRange;
        assert!(err.source().is_none());
    }

    #[test]
    fn invalid_source_boundary_is_error() {
        use std::error::Error;
        assert!(InvalidSourceBoundary::OutOfBounds.source().is_none());
        assert!(
            InvalidSourceBoundary::NotCharacterBoundary
                .source()
                .is_none()
        );
    }
}
