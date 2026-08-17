use super::*;

#[test]
fn try_range_converts_checked_unicode_boundaries() {
    let index = SourceLineIndex::new("éx");
    let range = index.try_range(ByteRange::new(0, 2).unwrap()).unwrap();
    assert_eq!(range.start().line(), 1);
    assert!(index.try_range(ByteRange::new(1, 2).unwrap()).is_err());
}

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
