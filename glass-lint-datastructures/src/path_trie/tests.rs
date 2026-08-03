use super::*;
use crate::name::NameTable;

fn property(names: &mut NameTable, value: &str) -> PathSegment {
    PathSegment::Property(names.intern(value).expect("path test names fit"))
}

#[test]
fn shared_prefixes_are_canonical_and_index_free() {
    let mut paths = PathStore::new();
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
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let property_seg = paths
        .append(PathId::EMPTY, property(&mut names, "0"))
        .unwrap();
    let index = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    assert_ne!(property_seg, index);
}

#[test]
fn appending_shared_prefixes_does_not_duplicate_nodes() {
    let mut paths = PathStore::new();
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
    let paths = PathStore::new();
    assert_eq!(paths.first_index(PathId::EMPTY), None);
}

#[test]
fn invalid_path_returns_none() {
    let paths = PathStore::new();
    let invalid = PathId::for_store(u32::MAX, 0);
    assert_eq!(paths.first_index(invalid), None);
    assert_eq!(paths.without_first(invalid), None);
}

#[test]
fn path_handles_are_owned_by_their_store() {
    let mut first = PathStore::new();
    let mut second = PathStore::new();
    let path = first.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    let foreign_root = PathId::for_store(0, u64::MAX);

    assert!(first.is_valid(path));
    assert!(!second.is_valid(path));
    assert_eq!(second.append(path, PathSegment::Index(1)), None);
    assert!(!second.is_valid(foreign_root));
    assert_eq!(second.append(foreign_root, PathSegment::Index(1)), None);
}

#[test]
fn first_index_returns_index_for_index_segment() {
    let mut paths = PathStore::new();
    let idx = paths.append(PathId::EMPTY, PathSegment::Index(7)).unwrap();
    assert_eq!(paths.first_index(idx), Some(7));
}

#[test]
fn first_index_returns_none_for_property_segment() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let prop = paths
        .append(PathId::EMPTY, property(&mut names, "x"))
        .unwrap();
    assert_eq!(paths.first_index(prop), None);
}

#[test]
fn starts_with_matches_exact_path() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert!(paths.starts_with(a, a));
}

#[test]
fn starts_with_rejects_deeper_prefix() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    assert!(!paths.starts_with(a, ab));
}

#[test]
fn without_first_on_single_segment_returns_empty() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.without_first(a), Some(PathId::EMPTY));
}

#[test]
fn without_first_on_multi_segment() {
    let mut paths = PathStore::new();
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
    let paths = PathStore::new();
    assert_eq!(paths.without_first(PathId::EMPTY), None);
}

#[test]
fn concat_creates_correct_intermediate_paths() {
    let mut paths = PathStore::new();
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
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.concat(a, PathId::EMPTY), Some(a));
}

#[test]
fn concat_with_empty_prefix_returns_suffix() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert_eq!(paths.concat(PathId::EMPTY, a), Some(a));
}

#[test]
fn concat_with_buffer_reuses_scratch_buffer() {
    let mut paths = PathStore::new();
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
    let mut paths = PathStore::new();
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
    let paths = PathStore::new();
    assert_eq!(paths.node_count(), 1);
}

#[test]
fn invalid_id_rejection() {
    let mut paths = PathStore::new();
    let result = paths.append(PathId::for_store(u32::MAX, 0), PathSegment::Index(0));
    assert_eq!(result, None);
}

#[test]
fn parent_lookup() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    assert_eq!(paths.parent(ab), Some(a));
}

#[test]
fn parent_of_root_points_to_self() {
    let paths = PathStore::new();
    assert_eq!(paths.parent(PathId::EMPTY), Some(PathId::EMPTY));
}

#[test]
fn parent_of_invalid_is_none() {
    let paths = PathStore::new();
    assert_eq!(paths.parent(PathId::for_store(u32::MAX, 0)), None);
}

#[test]
fn find_edge_on_existing() {
    let mut names = NameTable::default();
    let mut store = PathStore::with_max_nodes(100);
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append(PathId::EMPTY, seg).unwrap();
    assert_eq!(store.find_edge(PathId::EMPTY, &seg), Some(id));
}

#[test]
fn find_edge_on_missing() {
    let mut names = NameTable::default();
    let store = PathStore::with_max_nodes(100);
    let seg = PathSegment::Property(names.intern("x").unwrap());
    assert_eq!(store.find_edge(PathId::EMPTY, &seg), None);
}

#[test]
fn linked_children_preserve_typed_parent_and_deduplicate() {
    let mut source = PathStore::new();
    let parent = source.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
    let link = source.link(parent).unwrap();
    let mut overlay = PathStore::with_max_nodes(10);
    let segment = PathSegment::Index(1);

    let child = overlay.append_linked(link, segment).unwrap();
    assert_eq!(overlay.append_linked(link, segment), Some(child));
    assert_eq!(overlay.find_linked_edge(link, &segment), Some(child));
    assert_eq!(overlay.parent_ref(child), Some(ParentRef::Linked(link)));
    assert_eq!(overlay.depth(child), Some(2));
}

#[test]
fn linking_rejects_paths_owned_by_another_store() {
    let mut source = PathStore::new();
    let other = PathStore::new();
    let path = source.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();

    assert!(other.link(path).is_none());
}

#[test]
fn collect_segments_multi_segment() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let abc = paths.append(ab, property(&mut names, "c")).unwrap();
    let mut buf = Vec::new();
    paths.collect_segments(abc, &mut buf).unwrap();
    assert_eq!(buf.len(), 3);
    assert_eq!(buf[0], PathSegment::Property(names.lookup("a").unwrap()));
    assert_eq!(buf[1], PathSegment::Property(names.lookup("b").unwrap()));
    assert_eq!(buf[2], PathSegment::Property(names.lookup("c").unwrap()));
}

#[test]
fn collect_segments_on_root_returns_empty() {
    let paths = PathStore::new();
    let mut buf = vec![PathSegment::Index(99)];
    paths.collect_segments(PathId::EMPTY, &mut buf).unwrap();
    assert!(buf.is_empty());
}

#[test]
fn first_segment_of_returns_deepest_ancestor() {
    let mut store = PathStore::with_max_nodes(100);
    let mut names = NameTable::default();
    let seg_a = PathSegment::Property(names.intern("a").unwrap());
    let seg_b = PathSegment::Property(names.intern("b").unwrap());
    let a = store.append(PathId::EMPTY, seg_a).unwrap();
    let ab = store.append(a, seg_b).unwrap();
    let seg_c = PathSegment::Property(names.intern("c").unwrap());
    let abc = store.append(ab, seg_c).unwrap();
    assert_eq!(store.first_segment_of(abc), Some(&seg_a));
}

#[test]
fn first_segment_of_root_returns_none() {
    let store = PathStore::with_max_nodes(100);
    assert_eq!(store.first_segment_of(PathId::EMPTY), None);
}

#[test]
fn is_valid_returns_true_for_existing_ids() {
    let mut store = PathStore::with_max_nodes(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    let id = store.append(PathId::EMPTY, seg).unwrap();
    assert!(store.is_valid(id));
}

#[test]
fn is_valid_returns_false_for_out_of_range() {
    let store = PathStore::with_max_nodes(100);
    assert!(!store.is_valid(PathId::for_store(999, 0)));
}

#[test]
fn starts_with_empty_prefix() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    assert!(paths.starts_with(a, PathId::EMPTY));
}

#[test]
fn max_nodes_limits_growth() {
    let mut store = PathStore::with_max_nodes(2);
    let mut names = NameTable::default();
    let seg_a = PathSegment::Property(names.intern("a").unwrap());
    let seg_b = PathSegment::Property(names.intern("b").unwrap());
    assert!(store.append(PathId::EMPTY, seg_a).is_some());
    assert!(store.append(PathId::EMPTY, seg_b).is_none());
}

#[test]
fn max_nodes_accessor() {
    let store = PathStore::with_max_nodes(42);
    assert_eq!(store.max_nodes(), 42);
}

#[test]
fn segments_iterator_returns_all_segments() {
    let mut paths = PathStore::new();
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
    let collected: Vec<_> = paths.segments(abc).unwrap().collect();
    assert_eq!(collected, expected);
}

#[test]
fn segments_iterator_on_root_is_empty() {
    let paths = PathStore::new();
    assert_eq!(paths.segments(PathId::EMPTY).unwrap().count(), 0);
}

#[test]
fn segments_iterator_on_invalid_is_none() {
    let paths = PathStore::new();
    assert!(paths.segments(PathId::for_store(u32::MAX, 0)).is_none());
}

#[test]
fn exact_size_iterator() {
    let mut paths = PathStore::new();
    let mut names = NameTable::default();
    let a = paths
        .append(PathId::EMPTY, property(&mut names, "a"))
        .unwrap();
    let ab = paths.append(a, property(&mut names, "b")).unwrap();
    let mut iter = paths.segments(ab).unwrap();
    assert_eq!(iter.len(), 2);
    let _ = iter.next();
    assert_eq!(iter.len(), 1);
}

#[test]
fn path_id_empty_checks() {
    assert!(PathId::EMPTY.is_empty());
    assert!(!PathId::for_store(0, 1).is_empty());
}

#[test]
fn raw_nodes_and_edges_accessors() {
    let mut store = PathStore::with_max_nodes(100);
    let mut names = NameTable::default();
    let seg = PathSegment::Property(names.intern("x").unwrap());
    store.append(PathId::EMPTY, seg).unwrap();
    assert_eq!(store.node_count(), 2);
}
