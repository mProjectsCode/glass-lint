use glass_lint_datastructures::{PathId, PathSegment, PathStore};

use super::*;

fn make_frozen_paths() -> (PathStore, PathId, PathId, PathId) {
    let mut frozen = PathStore::new();
    let a = frozen.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    let b = frozen.append(a, PathSegment::Index(1)).unwrap();
    let c = frozen.append(a, PathSegment::Index(2)).unwrap();
    (frozen, a, b, c)
}

#[test]
fn frozen_path_is_referenced_without_copy() {
    let (frozen, a, _b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    let s_id = store.intern_frozen(a).unwrap();
    assert_eq!(s_id, SummaryPathId::from_frozen_path(a));
    assert!(s_id.is_frozen());
    assert_eq!(store.depth(s_id), Some(1));
}

#[test]
fn invalid_frozen_path_returns_none() {
    let empty = PathStore::new();
    let (frozen, a, _b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&empty);
    assert!(store.intern_frozen(a).is_none());
    assert!(store.intern_frozen(a).is_none());
    // a is valid in `frozen` but not in `empty` — validates that
    // cross-store IDs are rejected
    assert!(frozen.is_valid(a));
    assert!(!empty.is_valid(a));
}

#[test]
fn join_frozen_prefix_with_frozen_suffix_creates_overlay_node() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let prefix = store.intern_frozen(a).unwrap();
    let suffix = store.intern_frozen(b).unwrap();
    let joined = store.join(prefix, suffix).unwrap();
    assert!(!joined.is_frozen());
    assert!(!joined.is_empty());
    assert_eq!(store.depth(joined), Some(3));
}

#[test]
fn join_with_empty_is_identity() {
    let (frozen, a, _b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let prefix = store.intern_frozen(a).unwrap();
    assert_eq!(store.join(prefix, SummaryPathId::EMPTY), Some(prefix));
    assert_eq!(store.join(SummaryPathId::EMPTY, prefix), Some(prefix));
}

#[test]
fn frozen_reference_reused_by_multiple_summaries() {
    let (frozen, a, _b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    let id1 = store.intern_frozen(a).unwrap();
    let id2 = store.intern_frozen(a).unwrap();
    assert_eq!(id1, id2);
}

#[test]
fn starts_with_mixed_frozen_and_overlay() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    let ab = store.join(a_s, b_s).unwrap();
    assert!(store.starts_with(ab, a_s));
    assert!(store.starts_with(ab, ab));
}

#[test]
fn matches_frozen_checks_identity() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    assert!(store.matches_frozen(SummaryPathId::from_frozen_path(a), a));
    assert!(!store.matches_frozen(SummaryPathId::from_frozen_path(a), b,));
}

#[test]
fn starts_with_frozen_checks_prefix() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    let ab = store.join(a_s, b_s).unwrap();
    assert!(store.starts_with_frozen(ab, a));
    assert!(!store.starts_with_frozen(a_s, b));
}

#[test]
fn without_first_on_frozen() {
    let (frozen, _a, b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    let s_b = SummaryPathId::from_frozen_path(b);
    assert!(store.without_first(s_b).is_none());
}

#[test]
fn without_first_on_overlay() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    let ab = store.join(a_s, b_s).unwrap();
    let result = store.without_first(ab).unwrap();
    assert_eq!(result, b_s);
}

#[test]
fn owned_segments_on_frozen() {
    let (frozen, _a, b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    let s_b = SummaryPathId::from_frozen_path(b);
    let segs = store.owned_segments(s_b).unwrap();
    assert_eq!(segs, vec![PathSegment::Index(0), PathSegment::Index(1)]);
}

#[test]
fn owned_segments_on_joined_overlay() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    let ab = store.join(a_s, b_s).unwrap();
    let segs = store.owned_segments(ab).unwrap();
    assert_eq!(
        segs,
        vec![
            PathSegment::Index(0),
            PathSegment::Index(0),
            PathSegment::Index(1),
        ]
    );
}

#[test]
fn overlay_budget_exhaustion_fails_closed() {
    let (frozen, a, b, _c) = make_frozen_paths();
    let mut store = SummaryPathStore::with_max_nodes(&frozen, 2);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    assert!(store.join(a_s, b_s).is_none());
}

#[test]
fn empty_summary_path_has_no_segments() {
    let (frozen, _a, _b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    assert_eq!(store.depth(SummaryPathId::EMPTY), Some(0));
    assert_eq!(store.first_index(SummaryPathId::EMPTY), None);
    assert_eq!(store.without_first(SummaryPathId::EMPTY), None);
}

#[test]
fn first_index_on_frozen_and_overlay() {
    let (frozen, a, _b, _c) = make_frozen_paths();
    let store = SummaryPathStore::new(&frozen);
    let s_idx = SummaryPathId::from_frozen_path(a);
    assert_eq!(store.first_index(s_idx), Some(0));
}

#[test]
fn join_order_with_three_segments() {
    let (frozen, a, b, c) = make_frozen_paths();
    let mut store = SummaryPathStore::new(&frozen);
    let a_s = store.intern_frozen(a).unwrap();
    let b_s = store.intern_frozen(b).unwrap();
    let c_s = store.intern_frozen(c).unwrap();
    let ab = store.join(a_s, b_s).unwrap();
    let abc = store.join(ab, c_s).unwrap();
    assert_eq!(store.depth(abc), Some(5));
    assert!(store.starts_with(abc, a_s));
    let segs = store.owned_segments(abc).unwrap();
    assert_eq!(
        segs,
        vec![
            PathSegment::Index(0),
            PathSegment::Index(0),
            PathSegment::Index(1),
            PathSegment::Index(0),
            PathSegment::Index(2),
        ]
    );
}
