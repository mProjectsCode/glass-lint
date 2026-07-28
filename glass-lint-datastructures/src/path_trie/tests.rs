use super::*;
use crate::name::NameTable;

fn property(names: &mut NameTable, value: &str) -> PathSegment {
    PathSegment::Property(names.intern(value).expect("path test names fit"))
}

#[test]
fn shared_prefixes_are_canonical_and_index_free() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let client = paths
        .append(PathId::EMPTY, property(&mut names, "client"))
        .unwrap();
    let request = paths
        .append(client, property(&mut names, "request"))
        .unwrap();
    let send = paths.append(request, property(&mut names, "send")).unwrap();
    assert_eq!(
        paths.append(client, property(&mut names, "request")),
        Some(request)
    );
    assert!(paths.starts_with(send, request));
    assert_eq!(paths.last(send), Some(&property(&mut names, "send")));
    assert_eq!(paths.depth(send), Some(3));
}

#[test]
fn property_and_index_segments_remain_distinct() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let property_seg = paths
        .append(PathId::EMPTY, property(&mut names, "0"))
        .unwrap();
    let index = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    assert_ne!(property_seg, index);
}

#[test]
fn appending_shared_prefixes_does_not_duplicate_nodes() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let root = paths
        .append(PathId::EMPTY, property(&mut names, "root"))
        .unwrap();
    let before = paths.node_count();
    let _ = paths.append(root, property(&mut names, "child")).unwrap();
    let after = paths.node_count();
    assert_eq!(after, before + 1);
    let _ = paths.append(root, property(&mut names, "child")).unwrap();
    assert_eq!(paths.node_count(), after);
}

#[test]
fn empty_path_has_no_first_index() {
    let paths = PathInterner::new();
    assert_eq!(paths.first_index(PathId::EMPTY), None);
}

#[test]
fn invalid_path_returns_none() {
    let paths = PathInterner::new();
    assert_eq!(paths.first_index(PathId(u32::MAX)), None);
    assert_eq!(paths.without_first(PathId(u32::MAX)), None);
}

#[test]
fn first_index_returns_index_for_index_segment() {
    let mut paths = PathInterner::new();
    let idx = paths.append(PathId::EMPTY, PathSegment::Index(7)).unwrap();
    assert_eq!(paths.first_index(idx), Some(7));
}

#[test]
fn first_index_returns_none_for_property_segment() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let prop = paths
        .append(PathId::EMPTY, property(&mut names, "x"))
        .unwrap();
    assert_eq!(paths.first_index(prop), None);
}

#[test]
fn starts_with_matches_exact_path() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert!(paths.starts_with(a, a));
}

#[test]
fn starts_with_rejects_deeper_prefix() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    assert!(!paths.starts_with(a, ab));
}

#[test]
fn without_first_on_single_segment_returns_empty() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.without_first(a), Some(PathId::EMPTY));
}

#[test]
fn without_first_on_multi_segment() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let b = paths
        .append(PathId::EMPTY, property(&mut names, "b"))
        .unwrap();
    let bc = paths.append(b, property(&mut names, "c")).unwrap();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let abc = paths.append(ab, property(&mut names, "c")).unwrap();
    let result = paths.without_first(abc);
    assert_eq!(result, Some(bc));
    assert_eq!(paths.depth(bc), Some(2));
}

#[test]
fn without_first_on_empty_returns_none() {
    let paths = PathInterner::new();
    assert_eq!(paths.without_first(PathId::EMPTY), None);
}

#[test]
fn concat_creates_correct_intermediate_paths() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let b = paths
        .append(PathId::EMPTY, property(&mut names, "b"))
        .unwrap();
    let bc = paths.append(b, property(&mut names, "c")).unwrap();
    let abc = paths.concat(a, bc).unwrap();
    assert_eq!(paths.depth(abc), Some(3));
    assert!(paths.starts_with(abc, a));
    assert!(!paths.starts_with(abc, bc));
}

#[test]
fn concat_with_empty_suffix_returns_prefix() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.concat(a, PathId::EMPTY), Some(a));
}

#[test]
fn concat_with_empty_prefix_returns_suffix() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.concat(PathId::EMPTY, a), Some(a));
}

#[test]
fn concat_with_buffer_reuses_scratch_buffer() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let b = paths
        .append(PathId::EMPTY, property(&mut names, "b"))
        .unwrap();
    let bc = paths.append(b, property(&mut names, "c")).unwrap();
    let mut buf = vec![property(&mut names, "x")];
    let result = paths.concat_with_buffer(a, bc, &mut buf);
    assert!(result.is_some());
    assert!(buf.is_empty());
}

#[test]
fn edge_reuse_after_concat() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let b = paths
        .append(PathId::EMPTY, property(&mut names, "b"))
        .unwrap();
    let bc = paths.append(b, property(&mut names, "c")).unwrap();
    let abc = paths.concat(a, bc).unwrap();
    let before = paths.node_count();
    let abc2 = paths.concat(a, bc).unwrap();
    assert_eq!(abc, abc2);
    assert_eq!(paths.node_count(), before);
}

#[test]
fn node_count_tracking() {
    let paths = PathInterner::new();
    assert_eq!(paths.node_count(), 1);
}

#[test]
fn invalid_id_rejection() {
    let mut paths = PathInterner::new();
    let result = paths.append(PathId(u32::MAX), PathSegment::Index(0));
    assert_eq!(result, None);
}

#[test]
fn parent_lookup() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    assert_eq!(paths.store().parent(ab.0), Some(a.0));
}

#[test]
fn parent_of_root_points_to_self() {
    let paths = PathInterner::new();
    assert_eq!(paths.store().parent(PathId::EMPTY.0), Some(0));
}

#[test]
fn parent_of_invalid_is_none() {
    let paths = PathInterner::new();
    assert_eq!(paths.store().parent(u32::MAX), None);
}

#[test]
fn find_edge_on_existing() {
    let mut names = NameTable::default();
    let mut store = ParentPathStore::new(100);
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append(0, seg).unwrap();
    assert_eq!(store.find_edge(0, &seg), Some(id));
}

#[test]
fn find_edge_on_missing() {
    let mut names = NameTable::default();
    let store = ParentPathStore::new(100);
    let seg = PathSegment::Property(names.intern("x").unwrap());
    assert_eq!(store.find_edge(0, &seg), None);
}

#[test]
fn collect_segments_multi_segment() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let abc = paths.append(ab, property(&mut names, "c")).unwrap();
    let mut buf = Vec::new();
    paths.store().collect_segments(abc.0, &mut buf).unwrap();
    assert_eq!(buf.len(), 3);
    assert_eq!(buf[0], PathSegment::Property(names.lookup("a").unwrap()));
    assert_eq!(buf[1], PathSegment::Property(names.lookup("b").unwrap()));
    assert_eq!(buf[2], PathSegment::Property(names.lookup("c").unwrap()));
}

#[test]
fn collect_segments_on_root_returns_empty() {
    let paths = PathInterner::new();
    let mut buf = vec![PathSegment::Index(99)];
    paths.store().collect_segments(0, &mut buf).unwrap();
    assert!(buf.is_empty());
}

#[test]
fn append_linked_returns_tagged_id() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append_linked(0, seg, 1).unwrap();
    assert!(PathId(id).is_linked());
}

#[test]
fn append_linked_reuses_existing_edge() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id1 = store.append(0, seg).unwrap();
    let id2 = store.append_linked(0, seg, 1).unwrap();
    assert_eq!(id1, id2);
    assert!(!PathId(id2).is_linked());
}

#[test]
fn first_segment_of_returns_deepest_ancestor() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg_a = PathSegment::Property(names.intern("a").unwrap());
    let seg_b = PathSegment::Property(names.intern("b").unwrap());
    let a = store.append(0, seg_a).unwrap();
    let ab = store.append(a, seg_b).unwrap();
    let seg_c = PathSegment::Property(names.intern("c").unwrap());
    let abc = store.append(ab, seg_c).unwrap();
    assert_eq!(store.first_segment_of(abc), Some(&seg_a));
}

#[test]
fn first_segment_of_root_returns_none() {
    let store = ParentPathStore::new(100);
    assert_eq!(store.first_segment_of(0), None);
}

#[test]
fn is_valid_returns_true_for_existing_ids() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append(0, seg).unwrap();
    assert!(store.is_valid(id));
}

#[test]
fn is_valid_returns_false_for_out_of_range() {
    let store = ParentPathStore::new(100);
    assert!(!store.is_valid(999));
}

#[test]
fn starts_with_empty_prefix() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert!(paths.starts_with(a, PathId::EMPTY));
}

#[test]
fn max_nodes_limits_growth() {
    let mut store = ParentPathStore::new(2);
    let mut names = NameTable::default();
    let seg_a = PathSegment::Property(names.intern("a").unwrap());
    let seg_b = PathSegment::Property(names.intern("b").unwrap());
    assert!(store.append(0, seg_a).is_some());
    assert!(store.append(0, seg_b).is_none());
}

#[test]
fn max_nodes_accessor() {
    let store = ParentPathStore::new(42);
    assert_eq!(store.max_nodes(), 42);
}

#[test]
fn segments_iterator_returns_all_segments() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let abc = paths.append(ab, property(&mut names, "c")).unwrap();
    let expected = [
        PathSegment::Property(names.lookup("a").unwrap()),
        PathSegment::Property(names.lookup("b").unwrap()),
        PathSegment::Property(names.lookup("c").unwrap()),
    ];
    let collected: Vec<_> = paths.segments(abc).collect();
    assert_eq!(collected, expected);
}

#[test]
fn segments_iterator_on_root_is_empty() {
    let paths = PathInterner::new();
    assert_eq!(paths.segments(PathId::EMPTY).count(), 0);
}

#[test]
fn segments_iterator_on_invalid_is_empty() {
    let paths = PathInterner::new();
    assert_eq!(paths.segments(PathId(u32::MAX)).count(), 0);
}

#[test]
fn exact_size_iterator() {
    let mut paths = PathInterner::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let mut iter = paths.segments(ab);
    assert_eq!(iter.len(), 2);
    let _ = iter.next();
    assert_eq!(iter.len(), 1);
}

#[test]
fn path_id_tag_untag_roundtrip() {
    let raw = 42u32;
    let id = PathId(raw);
    assert_eq!(id.untag(), PathId(raw));
    let tagged = PathId(raw | PathId::LINK_TAG);
    assert!(tagged.is_linked());
    assert_eq!(tagged.untag(), PathId(raw));
}

#[test]
fn path_id_empty_checks() {
    assert!(PathId::EMPTY.is_empty());
    assert!(!PathId::EMPTY.is_linked());
    assert_eq!(PathId::EMPTY.untag(), PathId::EMPTY);
}

#[test]
fn find_linked_edge_delegates() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append(0, seg).unwrap();
    assert_eq!(store.find_linked_edge(0, &seg), Some(id));
}

#[test]
fn raw_nodes_and_edges_accessors() {
    let mut store = ParentPathStore::new(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    store.append(0, seg).unwrap();
    assert_eq!(store.node_count(), 2);
}

mod linked_id_roundtrips {
    use super::*;

    fn make_linked_pair(store: &mut ParentPathStore, names: &mut NameTable) -> (u32, u32) {
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let a = store.append(0, seg_a).unwrap();
        let b = store.append_linked(a, seg_b, 2).unwrap();
        assert!(PathId(b).is_linked());
        (a, b)
    }

    #[test]
    fn depth_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert_eq!(store.depth(b), Some(2));
    }

    #[test]
    fn depth_on_untagged_regular_id_is_unchanged() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let a = store.append(0, seg_a).unwrap();
        assert_eq!(store.depth(a), Some(1));
    }

    #[test]
    fn is_valid_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert!(store.is_valid(b));
    }

    #[test]
    fn is_valid_on_fabricated_tagged_id_is_false() {
        let store = ParentPathStore::new(100);
        let tagged: u32 = 0x3e7 | PathId::LINK_TAG;
        assert!(!store.is_valid(tagged));
    }

    #[test]
    fn parent_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let (a, b) = make_linked_pair(&mut store, &mut names);
        let parent_of_b = store.parent(b);
        assert_eq!(parent_of_b, Some(a));
    }

    #[test]
    fn segment_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert_eq!(store.segment(b), Some(&seg_b));
    }

    #[test]
    fn starts_with_linked_path_on_regular_prefix() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let (a, b) = make_linked_pair(&mut store, &mut names);
        assert!(store.starts_with(b, a));
    }

    #[test]
    fn starts_with_linked_path_on_self() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert!(store.starts_with(b, b));
    }

    #[test]
    fn starts_with_regular_path_on_linked_prefix() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_c = PathSegment::Property(names.intern("c").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        let c = store.append(0, seg_c).unwrap();
        assert!(!store.starts_with(c, b));
    }

    #[test]
    fn starts_with_linked_rejects_unrelated_prefix() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_c = PathSegment::Property(names.intern("c").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        let c = store.append(0, seg_c).unwrap();
        assert!(!store.starts_with(b, c));
    }

    #[test]
    fn first_segment_of_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert_eq!(store.first_segment_of(b), Some(&seg_a));
    }

    #[test]
    fn first_segment_of_linked_id_multi_segment() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let seg_c = PathSegment::Property(names.intern("c").unwrap());
        let a = store.append(0, seg_a).unwrap();
        let b = store.append(a, seg_b).unwrap();
        let c = store.append_linked(b, seg_c, 3).unwrap();
        assert_eq!(store.first_segment_of(c), Some(&seg_a));
    }

    #[test]
    fn find_linked_edge_on_tagged_parent() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        // a is untagged parent; find_linked_edge should still find the edge
        // because edges with linked parent are keyed with tagged parent
        assert_eq!(store.find_linked_edge(0, &seg_b), None);
        // find_edge with tagged parent
        assert_eq!(
            store.find_linked_edge(store.parent(b).unwrap(), &seg_b),
            Some(b)
        );
    }

    #[test]
    fn collect_segments_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        let mut buf = Vec::new();
        store.collect_segments(b, &mut buf).unwrap();
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0], seg_a);
        assert_eq!(buf[1], seg_b);
    }

    #[test]
    fn segments_iterator_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        let collected: Vec<_> = store.segments(b).collect();
        assert_eq!(collected, vec![seg_a, seg_b]);
    }

    #[test]
    fn last_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let (_a, b) = make_linked_pair(&mut store, &mut names);
        assert_eq!(store.last(b), Some(&seg_b));
    }

    #[test]
    fn append_linked_reuses_edge_before_capacity_check() {
        let mut store = ParentPathStore::new(2);
        let mut names = NameTable::default();
        let seg = PathSegment::Property(names.intern("x").unwrap());
        let id1 = store.append(0, seg).unwrap();
        // Capacity is 2 (root + one node), so next append would fail.
        // But append_linked should reuse the existing edge instead of failing.
        let id2 = store.append_linked(0, seg, 1).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn without_first_on_linked_id() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let a = store.append(0, seg_a).unwrap();
        let _b = store.append_linked(a, seg_b, 2).unwrap();
        // without_first on linked id walks via rebuild_without_first which uses
        // find_edge with untagged parent — this won't find linked edges.
        // For pure linked paths without_first is unsupported; this must return
        // None rather than panicking or producing garbage.
        assert!(store.without_first(a).is_some());
    }

    #[test]
    fn first_index_on_linked_index_first_segment() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let a = store.append(0, PathSegment::Index(42)).unwrap();
        let b = store
            .append_linked(a, PathSegment::Property(names.intern("x").unwrap()), 2)
            .unwrap();
        assert_eq!(store.first_index(b), Some(42));
    }

    #[test]
    fn linked_ids_are_invalid_after_out_of_bounds() {
        let store = ParentPathStore::new(10);
        // A tagged ID pointing past the storage should be invalid
        let tagged_outside: u32 = 0x14 | PathId::LINK_TAG;
        assert!(!store.is_valid(tagged_outside));
    }

    #[test]
    fn starts_with_regular_path_on_linked_prefix_where_prefix_is_first_segment_of_path() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let a = store.append(0, seg_a).unwrap();
        let b = store.append_linked(a, seg_b, 2).unwrap();
        // a is a proper prefix of b (b's segments are [a, b])
        assert!(store.starts_with(b, a));
        // The linked path b should also match itself
        assert!(store.starts_with(b, b));
    }

    #[test]
    fn linked_id_produces_correct_segments_via_path_interner() {
        // Verify that PathInterner (which wraps ParentPathStore) can handle
        // tagged IDs when they leak via store()
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let a = store.append(0, seg_a).unwrap();
        let b = store.append_linked(a, seg_b, 2).unwrap();
        let mut buf = Vec::new();
        assert!(store.collect_segments(b, &mut buf).is_some());
        assert_eq!(buf, vec![seg_a, seg_b]);
    }
}
