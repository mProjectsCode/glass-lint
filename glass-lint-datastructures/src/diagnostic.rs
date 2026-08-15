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
mod tests;
