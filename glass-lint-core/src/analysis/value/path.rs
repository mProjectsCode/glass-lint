//! Bounded prefix-interned paths used by semantic projections.
//!
//! A path is stored leaf-first as parent links, but public operations expose
//! segments in source order. The interner is the single place that translates
//! between those representations.
//!
//! Shared prefixes are canonicalized by `(parent, segment)`, which bounds
//! duplicate storage and makes path IDs suitable for equality and flow maps.

#![allow(dead_code)]

use std::collections::HashMap;

const MAX_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Canonical ID of a path node; zero is the empty path.
pub(in crate::analysis) struct PathId(u32);

impl PathId {
    /// Sentinel representing no path segments.
    pub(in crate::analysis) const EMPTY: Self = Self(0);

    /// Whether this ID denotes the empty path.
    pub(in crate::analysis) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    fn index(self) -> Option<usize> {
        usize::try_from(self.0)
            .ok()
            .filter(|index| *index < MAX_PATH_NODES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// One static property or numeric index path segment.
pub(in crate::analysis) enum PathSegment {
    /// Named property access.
    Property(String),
    /// Array/index access kept distinct from a property named by digits.
    Index(u32),
}

#[derive(Debug, Clone)]
/// Parent-linked node in the canonical path trie.
struct PathNode {
    /// Parent path ID.
    parent: PathId,
    /// Segment appended at this node, absent only for the root.
    segment: Option<PathSegment>,
    /// Number of segments from the root.
    depth: u32,
}

#[derive(Debug, Default)]
/// Bounded canonical interner for static member/index paths.
pub(in crate::analysis) struct PathInterner {
    /// Parent-linked path nodes, with node zero as the empty path.
    nodes: Vec<PathNode>,
    /// Canonical edge lookup for shared prefixes.
    by_edge: HashMap<(PathId, PathSegment), PathId>,
}

impl PathInterner {
    /// Create an interner containing only the empty root node.
    pub(in crate::analysis) fn new() -> Self {
        Self {
            nodes: vec![PathNode {
                parent: PathId::EMPTY,
                segment: None,
                depth: 0,
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
            segment: Some(segment.clone()),
            depth,
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
    pub(in crate::analysis) fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.nodes.get(path.index()?)?.segment.as_ref()
    }

    /// Return the parent path ID of a valid path.
    pub(in crate::analysis) fn parent(&self, path: PathId) -> Option<PathId> {
        self.nodes.get(path.index()?).map(|node| node.parent)
    }

    /// Iterate segments in source/root-to-leaf order.
    pub(in crate::analysis) fn iter_segments(
        &self,
        path: PathId,
    ) -> impl DoubleEndedIterator<Item = &PathSegment> {
        self.nodes_for(path)
            .into_iter()
            .filter_map(|node| node.segment.as_ref())
    }

    /// Return the first segment when it is an array/index segment.
    pub(in crate::analysis) fn first_index(&self, path: PathId) -> Option<u32> {
        match self.segments(path)?.first()? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    /// Remove the first segment and recover the canonical remaining path.
    pub(in crate::analysis) fn without_first(&self, path: PathId) -> Option<PathId> {
        let mut segments = self.segments(path)?;
        segments.first()?;
        segments.remove(0);
        let mut result = PathId::EMPTY;
        for segment in segments {
            result = self.by_edge.get(&(result, segment)).copied()?;
        }
        Some(result)
    }

    /// Append every segment of `suffix` to `prefix` through the interner.
    pub(in crate::analysis) fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let segments = self.segments(suffix)?;
        let mut result = prefix;
        for segment in segments {
            result = self.append(result, segment)?;
        }
        Some(result)
    }

    /// Return a path's segments from root to leaf.
    fn segments(&self, path: PathId) -> Option<Vec<PathSegment>> {
        let nodes = self.nodes_for(path);
        if !path.is_empty() && nodes.is_empty() {
            return None;
        }
        Some(
            nodes
                .into_iter()
                .filter_map(|node| node.segment.clone())
                .collect(),
        )
    }

    fn nodes_for(&self, path: PathId) -> Vec<&PathNode> {
        let mut nodes = Vec::new();
        let mut current = path;
        while !current.is_empty() {
            let Some(node) = self.nodes.get(current.index().unwrap_or(MAX_PATH_NODES)) else {
                return Vec::new();
            };
            nodes.push(node);
            current = node.parent;
        }
        nodes.reverse();
        nodes
    }

    #[cfg(test)]
    pub(in crate::analysis) fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_prefixes_are_canonical_and_index_free() {
        let mut paths = PathInterner::new();
        let client = paths
            .append(PathId::EMPTY, PathSegment::Property("client".into()))
            .unwrap();
        let request = paths
            .append(client, PathSegment::Property("request".into()))
            .unwrap();
        let send = paths
            .append(request, PathSegment::Property("send".into()))
            .unwrap();
        assert_eq!(
            paths.append(client, PathSegment::Property("request".into())),
            Some(request)
        );
        assert!(paths.starts_with(send, request));
        assert_eq!(
            paths.last(send),
            Some(&PathSegment::Property("send".into()))
        );
        assert_eq!(paths.depth(send), Some(3));
    }

    #[test]
    fn property_and_index_segments_remain_distinct() {
        let mut paths = PathInterner::new();
        let property = paths
            .append(PathId::EMPTY, PathSegment::Property("0".into()))
            .unwrap();
        let index = paths.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
        assert_ne!(property, index);
    }

    #[test]
    fn appending_shared_prefixes_does_not_duplicate_nodes() {
        let mut paths = PathInterner::new();
        let root = paths
            .append(PathId::EMPTY, PathSegment::Property("root".into()))
            .unwrap();
        let before = paths.node_count();
        let _ = paths
            .append(root, PathSegment::Property("child".into()))
            .unwrap();
        let after = paths.node_count();
        assert_eq!(after, before + 1);
        let _ = paths
            .append(root, PathSegment::Property("child".into()))
            .unwrap();
        assert_eq!(paths.node_count(), after);
    }
}
