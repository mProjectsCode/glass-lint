use glass_lint_datastructures::ByteRange;

use super::*;
use crate::analysis::facts::FactId;

fn span(start: u32, end: u32) -> ByteRange {
    ByteRange::new(start, end).unwrap()
}

fn occ(event_id: u32, start: u32, end: u32) -> Occurrence {
    Occurrence::new(FactId::from_test(event_id), span(start, end))
}

/// Reference merge: collect all occurrences then sort by
/// (event, start, end, bucket) — mirrors the old O(k·n) contract.
fn reference_merge<'a>(
    base: Option<&'a [Occurrence]>,
    overlay: &'a [&'a [Occurrence]],
) -> Vec<Occurrence> {
    let mut all: Vec<(Occurrence, usize)> = Vec::new();
    if let Some(b) = base {
        for &o in b {
            all.push((o, 0));
        }
    }
    for (bi, &bucket) in overlay.iter().enumerate() {
        let bucket_idx = usize::from(base.is_some()) + bi;
        for &o in bucket {
            all.push((o, bucket_idx));
        }
    }
    all.sort_by_key(|&(o, bi)| (o.event(), o.span().start(), o.span().end(), bi));
    all.into_iter().map(|(o, _)| o).collect()
}

#[test]
fn cursor_single_overlay_bucket() {
    let bucket = [occ(1, 5, 10), occ(2, 20, 30)];
    let overlays: Vec<&[Occurrence]> = vec![&bucket];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    assert_eq!(result, vec![occ(1, 5, 10), occ(2, 20, 30)]);
}

#[test]
fn cursor_base_only() {
    let base = [occ(1, 5, 10), occ(2, 20, 30)];
    let iter = BorrowedOccurrenceIter::new(Some(&base), &[]);
    let result: Vec<_> = iter.collect();
    assert_eq!(result, vec![occ(1, 5, 10), occ(2, 20, 30)]);
}

#[test]
fn cursor_empty_overlay() {
    let mut iter = BorrowedOccurrenceIter::new(None, &[]);
    assert!(iter.next().is_none());
}

#[test]
fn multi_merge_two_buckets() {
    let b0 = [occ(1, 5, 10), occ(3, 15, 20)];
    let b1 = [occ(2, 8, 12)];
    let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(None, &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_three_buckets_interleaved() {
    let b0 = [occ(1, 10, 20), occ(4, 40, 50)];
    let b1 = [occ(2, 10, 20), occ(3, 30, 40)];
    let b2 = [occ(1, 5, 10), occ(5, 50, 60)];
    let overlays: Vec<&[Occurrence]> = vec![&b0, &b1, &b2];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(None, &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_base_and_overlay() {
    let base = [occ(1, 5, 10)];
    let overlay = [occ(2, 8, 12)];
    let overlays: Vec<&[Occurrence]> = vec![&overlay];
    let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(Some(&base), &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_tie_break_by_bucket() {
    let b0 = [occ(1, 10, 20)];
    let b1 = [occ(1, 10, 20)];
    let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(None, &overlays);
    assert_eq!(result, expected);
    assert_eq!(result.len(), 2);
}

#[test]
fn multi_merge_with_empty_buckets() {
    let b0: [Occurrence; 0] = [];
    let b1 = [occ(1, 5, 10), occ(3, 25, 30)];
    let b2: [Occurrence; 0] = [];
    let b3 = [occ(2, 8, 12)];
    let overlays: Vec<&[Occurrence]> = vec![&b0, &b1, &b2, &b3];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(None, &overlays);
    assert_eq!(result, expected);
}

#[test]
fn ordered_selection_sorts_without_deduplicating_physical_events() {
    let selection = OccurrenceSelection::scanned(vec![
        occ(3, 30, 31),
        occ(1, 10, 11),
        occ(1, 10, 11),
        occ(2, 20, 21),
    ]);

    let ordered: Vec<_> = selection.into_ordered().collect();
    assert_eq!(
        ordered,
        vec![
            occ(1, 10, 11),
            occ(1, 10, 11),
            occ(2, 20, 21),
            occ(3, 30, 31)
        ]
    );
}

#[test]
fn ordered_normalized_selections_keep_their_lazy_order() {
    let values = [occ(1, 10, 11), occ(2, 20, 21)];
    let indexed: Vec<_> = OccurrenceSelection::indexed(&values)
        .into_ordered()
        .collect();
    assert_eq!(indexed, values);

    let borrowed = OccurrenceSelection::Borrowed(BorrowedOccurrenceIter::new(Some(&values), &[]));
    let borrowed: Vec<_> = borrowed.into_ordered().collect();
    assert_eq!(borrowed, values);
}

#[test]
fn multi_merge_base_is_empty() {
    let base: [Occurrence; 0] = [];
    let o0 = [occ(1, 5, 10), occ(2, 20, 30)];
    let o1 = [occ(3, 15, 20)];
    let overlays: Vec<&[Occurrence]> = vec![&o0, &o1];
    let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(Some(&base), &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_large_bucket_set() {
    let buckets: Vec<Vec<Occurrence>> = (0u32..20)
        .map(|i| {
            let event = (i % 7) + 1;
            let start = (i * 3) % 50;
            let end = start + (i % 10) + 5;
            vec![occ(event, start, end)]
        })
        .collect();
    let overlays: Vec<&[Occurrence]> = buckets.iter().map(Vec::as_slice).collect();
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(None, &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_base_and_multiple_overlays() {
    let base = [occ(2, 10, 20), occ(5, 50, 60)];
    let o0 = [occ(1, 5, 10), occ(3, 15, 20)];
    let o1 = [occ(4, 30, 40)];
    let overlays: Vec<&[Occurrence]> = vec![&o0, &o1];
    let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
    let result: Vec<_> = iter.collect();
    let expected = reference_merge(Some(&base), &overlays);
    assert_eq!(result, expected);
}

#[test]
fn multi_merge_preserves_duplicates() {
    let b0 = [occ(1, 5, 10), occ(2, 20, 30)];
    let b1 = [occ(1, 5, 10), occ(2, 20, 30)];
    let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
    let iter = BorrowedOccurrenceIter::new(None, &overlays);
    let result: Vec<_> = iter.collect();
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], occ(1, 5, 10));
    assert_eq!(result[1], occ(1, 5, 10));
    assert_eq!(result[2], occ(2, 20, 30));
    assert_eq!(result[3], occ(2, 20, 30));
}
