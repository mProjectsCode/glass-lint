use std::collections::HashMap;

use crate::name::NameId;

/// The default maximum number of path nodes in a [`ParentPathStore`].
pub const DEFAULT_MAX_PATH_NODES: usize = 1 << 20;

/// Canonical identifier of a path node; zero is the empty path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(pub u32);

impl PathId {
    /// Sentinel representing a path with no segments.
    pub const EMPTY: Self = Self(0);
    /// Bit tag reserved for summary overlay identifiers.
    ///
    /// Tagged IDs are valid only within the summary overlay that produced
    /// them.
    pub const LINK_TAG: u32 = 1 << 31;

    /// Whether this is the empty path.
    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    /// Whether this ID belongs to a summary overlay (tagged).
    pub fn is_linked(self) -> bool {
        self.0 & Self::LINK_TAG != 0
    }

    /// Strip the overlay tag, returning a canonical node index.
    #[must_use]
    pub fn untag(self) -> Self {
        Self(self.0 & !Self::LINK_TAG)
    }
}

/// One static property or numeric index path segment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// Named property access.
    Property(NameId),
    /// Array or index access, kept distinct from a property whose name
    /// happens to be digits.
    Index(u32),
}

/// Input types accepted when appending a segment to a path.
#[derive(Clone, Copy, Debug)]
pub enum PathSegmentInput<'a> {
    /// A property name provided as a string slice.
    Property(&'a str),
    /// A property name already interned as a [`NameId`].
    PropertyId(NameId),
    /// An array index.
    Index(u32),
}

/// A single node in a parent-linked path trie.
#[derive(Debug, Clone)]
pub struct PathNode {
    /// Parent path identifier (raw `u32`; may carry overlay tag in summary
    /// stores).
    pub parent: u32,
    /// Number of segments from the root.
    pub depth: u32,
    /// The segment that leads from `parent` to this node.
    ///
    /// `None` only for the root node (id 0).
    pub segment: Option<PathSegment>,
}

/// A bounded, parent-linked path trie.
///
/// Interns path segments so shared prefixes are stored exactly once.
/// Supports overlay tagging via [`PathId::LINK_TAG`] for summary stores.
#[derive(Debug)]
pub struct ParentPathStore {
    nodes: Vec<PathNode>,
    by_edge: HashMap<(u32, PathSegment), u32>,
    max_nodes: usize,
}

impl ParentPathStore {
    /// Creates a new store with a capacity limit.
    ///
    /// The root node (id 0) is created immediately.
    pub fn new(max_nodes: usize) -> Self {
        Self {
            nodes: vec![PathNode {
                parent: 0,
                depth: 0,
                segment: None,
            }],
            by_edge: HashMap::new(),
            max_nodes,
        }
    }

    /// Returns `true` if `id` is a valid (unsigned integer) index into the
    /// node vector.  Note that this does **not** strip the link tag, so a
    /// tagged ID from another store will return `false`.
    pub fn is_valid(&self, id: u32) -> bool {
        let idx = id as usize;
        idx < self.nodes.len()
    }

    /// Appends a child under `parent` for the given `segment`.
    ///
    /// Returns the existing ID if the edge already exists.  Returns `None`
    /// if `parent` is invalid or the store has reached its capacity.
    pub fn append(&mut self, parent: u32, segment: PathSegment) -> Option<u32> {
        if !self.is_valid(parent) {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment.clone())) {
            return Some(*path);
        }
        if self.nodes.len() >= self.max_nodes {
            return None;
        }
        let id = u32::try_from(self.nodes.len()).ok()?;
        let depth = self.nodes[parent as usize].depth.checked_add(1)?;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment.clone()),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    /// Appends a child whose parent may be a tagged ID owned by a linked
    /// overlay.
    ///
    /// The returned ID is tagged with [`PathId::LINK_TAG`].  This avoids
    /// adding the linked overlay's nodes to the canonical store while still
    /// providing a valid lookup key within the overlay.
    pub fn append_linked(&mut self, parent: u32, segment: PathSegment, depth: u32) -> Option<u32> {
        if self.node_count() >= self.max_nodes {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment.clone())) {
            return Some(*path);
        }
        let id = u32::try_from(self.nodes.len()).ok()? | PathId::LINK_TAG;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment.clone()),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    /// Returns the depth (number of segments) of the node at `id`.
    pub fn depth(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.depth)
    }

    /// Returns the parent of the node at `id`.
    pub fn parent(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.parent)
    }

    /// Returns `true` if `path` starts with the same segments as `prefix`.
    pub fn starts_with(&self, path: u32, prefix: u32) -> bool {
        let Some(path_depth) = self.depth(path) else {
            return false;
        };
        let Some(prefix_depth) = self.depth(prefix) else {
            return false;
        };
        if prefix_depth > path_depth {
            return false;
        }
        let mut current = PathId(path).untag().0;
        for _ in 0..(path_depth - prefix_depth) {
            let index = current as usize;
            let Some(node) = self.nodes.get(index) else {
                return false;
            };
            current = node.parent;
        }
        current == prefix
    }

    /// Returns the segment that leads into the node at `id`.
    ///
    /// Returns `None` for the root node or invalid ids.
    pub fn segment(&self, id: u32) -> Option<&PathSegment> {
        let id = PathId(id).untag().0 as usize;
        if id == 0 {
            return None;
        }
        self.nodes.get(id)?.segment.as_ref()
    }

    /// Returns the segment of the first (deepest) ancestor.
    ///
    /// For a path `a.b.c`, returns `Some(Property("a"))`.
    pub fn first_segment_of(&self, id: u32) -> Option<&PathSegment> {
        let mut current = PathId(id).untag().0;
        let mut last = None;
        while current != 0 {
            let node = self.nodes.get(current as usize)?;
            last = Some(self.segment(current)?);
            current = node.parent;
        }
        last
    }

    /// Looks up an edge by parent and segment in the canonical store.
    pub fn find_edge(&self, parent: u32, segment: &PathSegment) -> Option<u32> {
        self.by_edge.get(&(parent, segment.clone())).copied()
    }

    /// Looks up an edge by parent and segment.
    ///
    /// Currently identical to [`find_edge`](Self::find_edge); kept as a
    /// separate method for symmetry with the linked overlay API.
    pub fn find_linked_edge(&self, parent: u32, segment: &PathSegment) -> Option<u32> {
        self.find_edge(parent, segment)
    }

    /// Collects the full segment sequence for `id` into `buf`.
    ///
    /// The buffer is cleared first, then filled with segments from root to
    /// `id`.
    pub fn collect_segments(&self, id: u32, buf: &mut Vec<PathSegment>) -> Option<()> {
        buf.clear();
        let mut current = id;
        while current != 0 {
            let node = self.nodes.get(current as usize)?;
            buf.push(self.segment(current)?.clone());
            current = node.parent;
        }
        buf.reverse();
        Some(())
    }

    /// The total number of nodes (including the root).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The maximum number of nodes allowed.
    pub fn max_nodes(&self) -> usize {
        self.max_nodes
    }

    /// The last (leaf-most) segment of the path at `id`.
    pub fn last(&self, id: u32) -> Option<&PathSegment> {
        self.segment(id)
    }

    /// If the path starts with an index segment, returns its value.
    pub fn first_index(&self, id: u32) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    /// Returns the path with the first segment removed.
    ///
    /// For a path `a.b.c`, returns the id for `b.c`.  Returns `Some(0)` if
    /// the path has only one segment.
    pub fn without_first(&self, id: u32) -> Option<u32> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    /// Recursive helper for [`without_first`](Self::without_first).
    fn rebuild_without_first(&self, id: u32) -> Option<u32> {
        let node = self.nodes.get(id as usize)?;
        let segment = self.segment(id)?;
        if node.parent == 0 {
            return Some(0);
        }
        let parent = self.rebuild_without_first(node.parent)?;
        self.find_edge(parent, segment)
    }

    /// Returns an iterator over the segments of `id` from root to leaf.
    ///
    /// Returns an empty iterator for the root node or invalid ids.
    pub fn segments(&self, id: u32) -> PathSegments {
        let mut collected = Vec::new();
        self.collect_segments(id, &mut collected);
        PathSegments {
            segments: collected,
            index: 0,
        }
    }

    /// Access the raw node vector (read-only).
    ///
    /// Useful for iteration and debugging.
    pub fn raw_nodes(&self) -> &[PathNode] {
        &self.nodes
    }

    /// Access the edge map (read-only).
    pub fn raw_edges(&self) -> &HashMap<(u32, PathSegment), u32> {
        &self.by_edge
    }
}

/// An iterator over the segments of a path from root to leaf.
///
/// Yields owned [`PathSegment`] values.  Created by
/// [`ParentPathStore::segments`] and [`PathInterner::segments`].
#[derive(Clone, Debug)]
pub struct PathSegments {
    segments: Vec<PathSegment>,
    index: usize,
}

impl Iterator for PathSegments {
    type Item = PathSegment;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.segments.get(self.index)?;
        self.index += 1;
        Some(result.clone())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.segments.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PathSegments {}

/// A high-level path interner backed by a [`ParentPathStore`].
///
/// Provides a convenient API for interned paths using [`PathId`] and
/// [`PathSegment`] types, with support for concatenation and prefix queries.
#[derive(Debug)]
pub struct PathInterner {
    store: ParentPathStore,
}

impl PathInterner {
    /// Creates a new interner with the default maximum node count.
    pub fn new() -> Self {
        Self {
            store: ParentPathStore::new(DEFAULT_MAX_PATH_NODES),
        }
    }

    /// Appends `segment` under `parent`, returning the existing or new id.
    pub fn append(&mut self, parent: PathId, segment: PathSegment) -> Option<PathId> {
        self.store.append(parent.0, segment).map(PathId)
    }

    /// Returns the depth of `path`.
    pub fn depth(&self, path: PathId) -> Option<u32> {
        self.store.depth(path.0)
    }

    /// Returns `true` if `path` starts with `prefix`.
    pub fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
        self.store.starts_with(path.0, prefix.0)
    }

    /// Returns a shared reference to the underlying store.
    pub fn store(&self) -> &ParentPathStore {
        &self.store
    }

    /// Returns the last (leaf-most) segment of `path`.
    pub fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.store.last(path.0)
    }

    /// If the path starts with an index segment, returns its value.
    pub fn first_index(&self, path: PathId) -> Option<u32> {
        self.store.first_index(path.0)
    }

    /// Returns the path with the first segment removed.
    pub fn without_first(&self, path: PathId) -> Option<PathId> {
        self.store.without_first(path.0).map(PathId)
    }

    /// Concatenates `prefix` and `suffix`, reusing `buf` as scratch space.
    pub fn concat_with_buffer(
        &mut self,
        prefix: PathId,
        suffix: PathId,
        buf: &mut Vec<PathSegment>,
    ) -> Option<PathId> {
        self.store.collect_segments(suffix.0, buf)?;
        let mut result = prefix;
        for segment in buf.drain(..) {
            result = self.append(result, segment)?;
        }
        Some(result)
    }

    /// Concatenates `prefix` and `suffix`.
    pub fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let mut buf = Vec::new();
        self.concat_with_buffer(prefix, suffix, &mut buf)
    }

    /// Returns an iterator over the segments of `path` from root to leaf.
    pub fn segments(&self, path: PathId) -> PathSegments {
        self.store.segments(path.0)
    }

    /// The total number of nodes in the store.
    pub fn node_count(&self) -> usize {
        self.store.node_count()
    }
}

impl Default for PathInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
        // The root node stores parent=0, so parent(0) returns Some(0)
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
        let id = store.append(0, seg.clone()).unwrap();
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
        let id1 = store.append(0, seg.clone()).unwrap();
        let id2 = store.append_linked(0, seg, 1).unwrap();
        // append_linked returns the *un-tagged* canonical id if the edge exists
        assert_eq!(id1, id2);
        assert!(!PathId(id2).is_linked());
    }

    #[test]
    fn first_segment_of_returns_deepest_ancestor() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        let a = store.append(0, seg_a.clone()).unwrap();
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
        // Empty prefix (PathId::EMPTY) should match anything
        assert!(paths.starts_with(a, PathId::EMPTY));
    }

    #[test]
    fn max_nodes_limits_growth() {
        let mut store = ParentPathStore::new(2);
        let mut names = NameTable::default();
        let seg_a = PathSegment::Property(names.intern("a").unwrap());
        let seg_b = PathSegment::Property(names.intern("b").unwrap());
        // Root (1 node) + first append (2 nodes)
        assert!(store.append(0, seg_a).is_some());
        // max_nodes=2, so second distinct append should fail
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
        let id = store.append(0, seg.clone()).unwrap();
        assert_eq!(store.find_linked_edge(0, &seg), Some(id));
    }

    #[test]
    fn raw_nodes_and_edges_accessors() {
        let mut store = ParentPathStore::new(100);
        let mut names = NameTable::default();
        let seg = PathSegment::Property(names.intern("x").unwrap());
        store.append(0, seg).unwrap();
        assert_eq!(store.raw_nodes().len(), 2);
        assert_eq!(store.raw_edges().len(), 1);
    }
}
