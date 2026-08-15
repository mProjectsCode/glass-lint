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
    use hashbrown::HashSet;
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
