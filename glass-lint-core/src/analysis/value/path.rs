use std::collections::HashMap;

use crate::analysis::name::NameId;

const MAX_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Canonical ID of a path node; zero is the empty path.
pub(in crate::analysis) struct PathId(pub(in crate::analysis) u32);

impl PathId {
    /// Sentinel representing no path segments.
    pub(in crate::analysis) const EMPTY: Self = Self(0);
    /// Tag bit reserved for summary overlay IDs. Tagged IDs are valid only
    /// within the summary overlay that produced them.
    pub(in crate::analysis) const LINK_TAG: u32 = 1 << 31;

    pub(in crate::analysis) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    /// Whether this ID belongs to a summary overlay (tagged).
    #[allow(dead_code)]
    pub(in crate::analysis) fn is_linked(self) -> bool {
        self.0 & Self::LINK_TAG != 0
    }

    /// Strip the overlay tag, returning a canonical node index.
    pub(in crate::analysis) fn untag(self) -> Self {
        Self(self.0 & !Self::LINK_TAG)
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
/// Parent-linked node in a path trie.
pub(in crate::analysis) struct PathNode {
    /// Parent path ID (raw u32, may carry overlay tag in summary store).
    pub(in crate::analysis) parent: u32,
    /// Number of segments from the root.
    pub(in crate::analysis) depth: u32,
    /// Segment leading from `parent` to this node.
    pub(in crate::analysis) segment: Option<PathSegment>,
}

#[derive(Debug)]
/// Bounded parent-linked path trie shared by canonical and overlay stores.
pub(in crate::analysis) struct ParentPathStore {
    /// Parent-linked path nodes, with node zero as the empty path.
    pub(in crate::analysis) nodes: Vec<PathNode>,
    /// Addressable canonical edge lookup.
    pub(in crate::analysis) by_edge: HashMap<(u32, PathSegment), u32>,
    max_nodes: usize,
}

impl ParentPathStore {
    pub(in crate::analysis) fn new(max_nodes: usize) -> Self {
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

    pub(in crate::analysis) fn is_valid(&self, id: u32) -> bool {
        let idx = id as usize;
        idx < self.nodes.len()
    }

    pub(in crate::analysis) fn append(&mut self, parent: u32, segment: PathSegment) -> Option<u32> {
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

    /// Append a node whose parent may be an ID owned by a linked overlay.
    pub(in crate::analysis) fn append_linked(
        &mut self,
        parent: u32,
        segment: PathSegment,
        depth: u32,
    ) -> Option<u32> {
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

    pub(in crate::analysis) fn depth(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.depth)
    }

    pub(in crate::analysis) fn parent(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.parent)
    }

    pub(in crate::analysis) fn starts_with(&self, path: u32, prefix: u32) -> bool {
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

    pub(in crate::analysis) fn segment(&self, id: u32) -> Option<&PathSegment> {
        let id = PathId(id).untag().0 as usize;
        if id == 0 {
            return None;
        }
        self.nodes.get(id)?.segment.as_ref()
    }

    pub(in crate::analysis) fn first_segment_of(&self, id: u32) -> Option<&PathSegment> {
        let mut current = PathId(id).untag().0;
        let mut last = None;
        while current != 0 {
            let node = self.nodes.get(current as usize)?;
            last = Some(self.segment(current)?);
            current = node.parent;
        }
        last
    }

    #[cfg(test)]
    pub(in crate::analysis) fn first_index(&self, id: u32) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    pub(in crate::analysis) fn find_edge(&self, parent: u32, segment: &PathSegment) -> Option<u32> {
        self.by_edge.get(&(parent, segment.clone())).copied()
    }

    pub(in crate::analysis) fn find_linked_edge(
        &self,
        parent: u32,
        segment: &PathSegment,
    ) -> Option<u32> {
        self.find_edge(parent, segment)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn without_first(&self, id: u32) -> Option<u32> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    #[cfg(test)]
    fn rebuild_without_first(&self, id: u32) -> Option<u32> {
        let node = self.nodes.get(id as usize)?;
        let segment = self.segment(id)?;
        if node.parent == 0 {
            return Some(0);
        }
        let parent = self.rebuild_without_first(node.parent)?;
        self.find_edge(parent, segment)
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect_segments(
        &self,
        id: u32,
        buf: &mut Vec<PathSegment>,
    ) -> Option<()> {
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

    pub(in crate::analysis) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(in crate::analysis) fn max_nodes(&self) -> usize {
        self.max_nodes
    }
}

#[derive(Debug)]
/// Bounded canonical interner for static member/index paths.
pub(in crate::analysis) struct PathInterner {
    store: ParentPathStore,
}

impl PathInterner {
    /// Create an interner containing only the empty root node.
    pub(in crate::analysis) fn new() -> Self {
        Self {
            store: ParentPathStore::new(MAX_PATH_NODES),
        }
    }

    /// Append one segment, reusing a shared edge or failing at the node bound.
    pub(in crate::analysis) fn append(
        &mut self,
        parent: PathId,
        segment: PathSegment,
    ) -> Option<PathId> {
        self.store.append(parent.0, segment).map(PathId)
    }

    /// Return the segment depth of a valid path.
    #[allow(dead_code)]
    pub(in crate::analysis) fn depth(&self, path: PathId) -> Option<u32> {
        self.store.depth(path.0)
    }

    /// Whether `path` has `prefix` as its canonical root prefix.
    #[allow(dead_code)]
    pub(in crate::analysis) fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
        self.store.starts_with(path.0, prefix.0)
    }

    /// Borrow the underlying store (used by summary overlay for read-only
    /// access).
    pub(in crate::analysis) fn store(&self) -> &ParentPathStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::name::NameTable;

    trait PathInternerTestExt {
        fn last(&self, path: PathId) -> Option<&PathSegment>;
        fn first_index(&self, path: PathId) -> Option<u32>;
        fn without_first(&self, path: PathId) -> Option<PathId>;
        fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId>;
        fn concat_with_buffer(
            &mut self,
            prefix: PathId,
            suffix: PathId,
            buf: &mut Vec<PathSegment>,
        ) -> Option<PathId>;
        fn node_count(&self) -> usize;
    }

    impl PathInternerTestExt for PathInterner {
        fn last(&self, path: PathId) -> Option<&PathSegment> {
            self.store.segment(path.0)
        }

        fn first_index(&self, path: PathId) -> Option<u32> {
            self.store.first_index(path.0)
        }

        fn without_first(&self, path: PathId) -> Option<PathId> {
            self.store.without_first(path.0).map(PathId)
        }

        fn concat_with_buffer(
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

        fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
            let mut buf = Vec::new();
            self.concat_with_buffer(prefix, suffix, &mut buf)
        }

        fn node_count(&self) -> usize {
            self.store.node_count()
        }
    }

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
}
