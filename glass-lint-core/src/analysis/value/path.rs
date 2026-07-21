//! Bounded prefix-interned paths used by semantic projections.
//!
//! A path is stored leaf-first as parent links, but public operations expose
//! segments in source order. The interner is the single place that translates
//! between those representations.
//!
//! Shared prefixes are canonicalized by `(parent, segment)`, which bounds
//! duplicate storage and makes path IDs suitable for equality and flow maps.

use std::collections::HashMap;

use crate::analysis::name::NameId;

const MAX_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Canonical ID of a path node; zero is the empty path.
pub(in crate::analysis) struct PathId(u32);

impl PathId {
    /// Sentinel representing no path segments.
    pub(in crate::analysis) const EMPTY: Self = Self(0);

    pub(in crate::analysis) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    /// Whether this ID denotes the empty path.
    fn index(self) -> Option<usize> {
        usize::try_from(self.0)
            .ok()
            .filter(|index| *index < MAX_PATH_NODES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// One static property or numeric index path segment.
pub(in crate::analysis) enum PathSegment {
    /// Named property access.
    Property(NameId),
    /// Array/index access kept distinct from a property named by digits.
    Index(u32),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::analysis) enum PathSegmentInput<'a> {
    Property(&'a str),
    PropertyId(NameId),
    Index(u32),
}

#[derive(Debug, Clone)]
/// Parent-linked node in the canonical path trie.
struct PathNode {
    /// Parent path ID.
    parent: PathId,
    /// Number of segments from the root.
    depth: u32,
    /// Segment leading from `parent` to this node.
    segment: Option<PathSegment>,
}

#[derive(Debug, Default)]
/// Bounded canonical interner for static member/index paths.
pub(in crate::analysis) struct PathInterner {
    /// Parent-linked path nodes, with node zero as the empty path.
    nodes: Vec<PathNode>,
    /// Addressable canonical edge lookup. The node retains its segment for
    /// ID-to-segment queries, while this index avoids scanning sibling edges.
    by_edge: HashMap<(PathId, PathSegment), PathId>,
}

impl PathInterner {
    /// Create an interner containing only the empty root node.
    pub(in crate::analysis) fn new() -> Self {
        Self {
            nodes: vec![PathNode {
                parent: PathId::EMPTY,
                depth: 0,
                segment: None,
            }],
            by_edge: HashMap::new(),
        }
    }

    /// Append one segment, reusing a shared edge or failing at the node bound.
    pub(in crate::analysis) fn append(
        &mut self,
        parent: PathId,
        segment: PathSegment,
    ) -> Option<PathId> {
        let parent_index = parent.index()?;
        if parent_index >= self.nodes.len() {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment.clone())) {
            return Some(*path);
        }
        if self.nodes.len() >= MAX_PATH_NODES {
            return None;
        }
        let id = PathId(u32::try_from(self.nodes.len()).ok()?);
        let depth = self.nodes[parent_index].depth.checked_add(1)?;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment.clone()),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    /// Return the segment depth of a valid path.
    pub(in crate::analysis) fn depth(&self, path: PathId) -> Option<u32> {
        self.nodes.get(path.index()?).map(|node| node.depth)
    }

    /// Whether `path` has `prefix` as its canonical root prefix.
    pub(in crate::analysis) fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
        let Some(path_depth) = self.depth(path) else {
            return false;
        };
        let Some(prefix_depth) = self.depth(prefix) else {
            return false;
        };
        if prefix_depth > path_depth {
            return false;
        }
        let mut current = path;
        for _ in 0..(path_depth - prefix_depth) {
            let Some(index) = current.index() else {
                return false;
            };
            let Some(node) = self.nodes.get(index) else {
                return false;
            };
            current = node.parent;
        }
        current == prefix
    }

    /// Borrow the final segment of a valid non-empty path.
    #[cfg(test)]
    pub(in crate::analysis) fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.segment(path)
    }

    fn segment(&self, path: PathId) -> Option<&PathSegment> {
        let node = self.nodes.get(path.index()?)?;
        if path.is_empty() {
            return None;
        }
        node.segment.as_ref()
    }

    /// Walk the parent chain from `path` to the root and collect segments in
    /// source order into the caller-owned buffer. Returns `None` for invalid
    /// paths; returns `Some(())` (possibly with an empty buffer) for the empty
    /// root path.
    fn collect_segments(&self, path: PathId, buf: &mut Vec<PathSegment>) -> Option<()> {
        buf.clear();
        let mut current = path;
        while !current.is_empty() {
            let node = self.nodes.get(current.index()?)?;
            buf.push(self.segment(current)?.clone());
            current = node.parent;
        }
        buf.reverse();
        Some(())
    }

    /// Return a summary-owned projection of a valid path.
    pub(in crate::analysis) fn owned_segments(&self, path: PathId) -> Option<Vec<PathSegment>> {
        let mut segments = Vec::new();
        self.collect_segments(path, &mut segments)?;
        Some(segments)
    }

    /// Return the first segment of a path by walking directly to the
    /// leaf node. This avoids building a complete segment vector.
    fn first_segment_of(&self, path: PathId) -> Option<&PathSegment> {
        let mut current = path;
        let mut last = None;
        while !current.is_empty() {
            let node = self.nodes.get(current.index()?)?;
            last = Some(self.segment(current)?);
            current = node.parent;
        }
        last
    }

    /// Return the first index segment of a valid path, if the first
    /// segment is an array/index access.
    pub(in crate::analysis) fn first_index(&self, path: PathId) -> Option<u32> {
        match self.first_segment_of(path)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    /// Remove the first segment and recover the canonical remaining path.
    pub(in crate::analysis) fn without_first(&self, path: PathId) -> Option<PathId> {
        self.segment(path)?;
        self.rebuild_without_first(path)
    }

    fn find_edge(&self, parent: PathId, segment: &PathSegment) -> Option<PathId> {
        self.by_edge
            .get(&(parent, segment.clone()))
            .copied()
    }

    /// Rebuild the canonical suffix while unwinding the parent chain. This
    /// uses call-stack storage instead of materializing a segment vector.
    fn rebuild_without_first(&self, path: PathId) -> Option<PathId> {
        let node = self.nodes.get(path.index()?)?;
        let segment = self.segment(path)?;
        if node.parent.is_empty() {
            return Some(PathId::EMPTY);
        }
        let parent = self.rebuild_without_first(node.parent)?;
        self.find_edge(parent, segment)
    }

    /// Append every segment of `suffix` to `prefix` through the interner,
    /// reusing a caller-owned scratch buffer to avoid intermediate
    /// allocation.
    #[cfg(test)]
    pub(in crate::analysis) fn concat_with_buffer(
        &mut self,
        prefix: PathId,
        suffix: PathId,
        buf: &mut Vec<PathSegment>,
    ) -> Option<PathId> {
        self.collect_segments(suffix, buf)?;
        let mut result = prefix;
        for segment in buf.drain(..) {
            result = self.append(result, segment)?;
        }
        Some(result)
    }

    /// Append every segment of `suffix` to `prefix` through the interner.
    #[cfg(test)]
    pub(in crate::analysis) fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let mut buf = Vec::new();
        self.concat_with_buffer(prefix, suffix, &mut buf)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::name::NameTable;

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
        let property = paths
            .append(PathId::EMPTY, property(&mut names, "0"))
            .unwrap();
        let index = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
        assert_ne!(property, index);
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
        // Build "b.c" first so the edges exist for re-interning.
        let b = paths
            .append(PathId::EMPTY, property(&mut names, "b"))
            .unwrap();
        let bc = paths.append(b, property(&mut names, "c")).unwrap();
        // Now build "a.b.c" by appending to "a" using the "b.c" subtree.
        let a = paths
            .append(PathId::EMPTY, property(&mut names, "a"))
            .unwrap();
        let ab = paths.append(a, property(&mut names, "b")).unwrap();
        let abc = paths.append(ab, property(&mut names, "c")).unwrap();
        // without_first("a.b.c") strips the first segment "a",
        // then re-interns "b.c" from EMPTY.  The edges (EMPTY, "b")
        // and (b, "c") already exist, so this yields the same PathId
        // as `bc`.
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
        // concat("a", "b.c") = "a.b.c"
        let abc = paths.concat(a, bc).unwrap();
        assert_eq!(paths.depth(abc), Some(3));
        // "a.b.c" starts with "a" in the trie
        assert!(paths.starts_with(abc, a));
        // "a.b.c" does NOT start with "b.c" (different subtree)
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
        // Re-concatenating the same segments should reuse edges
        let abc2 = paths.concat(a, bc).unwrap();
        assert_eq!(abc, abc2);
        assert_eq!(paths.node_count(), before);
    }
}
