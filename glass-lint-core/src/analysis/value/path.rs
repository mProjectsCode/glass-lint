//! Bounded prefix-interned paths used by semantic projections.
//!
//! A path is stored leaf-first as parent links, but public operations expose
//! segments in source order. The interner is the single place that translates
//! between those representations.

#![allow(dead_code)]

use std::collections::HashMap;

const MAX_PATH_NODES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct PathId(u32);

impl PathId {
    pub(in crate::analysis) const EMPTY: Self = Self(0);

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
pub(in crate::analysis) enum PathSegment {
    Property(String),
    Index(u32),
}

#[derive(Debug, Clone)]
struct PathNode {
    parent: PathId,
    segment: Option<PathSegment>,
    depth: u32,
}

#[derive(Debug, Default)]
pub(in crate::analysis) struct PathInterner {
    nodes: Vec<PathNode>,
    by_edge: HashMap<(PathId, PathSegment), PathId>,
}

impl PathInterner {
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

    pub(in crate::analysis) fn depth(&self, path: PathId) -> Option<u32> {
        self.nodes.get(path.index()?).map(|node| node.depth)
    }

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

    pub(in crate::analysis) fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.nodes.get(path.index()?)?.segment.as_ref()
    }

    pub(in crate::analysis) fn first_index(&self, path: PathId) -> Option<u32> {
        match self.segments(path)?.first()? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

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
        let mut segments = Vec::new();
        let mut current = path;
        while !current.is_empty() {
            let node = self.nodes.get(current.index()?)?;
            segments.push(node.segment.as_ref()?.clone());
            current = node.parent;
        }
        segments.reverse();
        Some(segments)
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
