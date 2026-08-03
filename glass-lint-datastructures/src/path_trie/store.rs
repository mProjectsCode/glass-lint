use std::sync::atomic::{AtomicU64, Ordering};

use hashbrown::HashMap;

use crate::{PathId, PathSegment};

static NEXT_STORE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathLink {
    path: PathId,
    depth: u32,
}

impl PathLink {
    pub fn path(self) -> PathId {
        self.path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParentRef {
    Local(PathId),
    Linked(PathLink),
}

#[derive(Debug, Clone)]
struct PathNode {
    parent: ParentRef,
    depth: u32,
    segment: Option<PathSegment>,
}

#[derive(Debug)]
pub struct PathStore {
    nodes: Vec<PathNode>,
    by_edge: HashMap<(ParentRef, PathSegment), PathId>,
    max_nodes: usize,
    owner: u64,
}

impl PathStore {
    pub fn new() -> Self {
        Self::with_max_nodes(super::DEFAULT_MAX_PATH_NODES)
    }

    pub fn with_max_nodes(max_nodes: usize) -> Self {
        Self {
            nodes: vec![PathNode {
                parent: ParentRef::Local(PathId::EMPTY),
                depth: 0,
                segment: None,
            }],
            by_edge: HashMap::new(),
            max_nodes,
            owner: NEXT_STORE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    fn local_id(&self, id: PathId) -> Option<PathId> {
        if id == PathId::EMPTY {
            return Some(PathId::EMPTY);
        }
        (id.owner() == self.owner).then_some(id)
    }

    fn node(&self, id: PathId) -> Option<&PathNode> {
        let id = self.local_id(id)?;
        self.nodes.get(id.index() as usize)
    }

    pub fn is_valid(&self, id: PathId) -> bool {
        self.node(id).is_some()
    }

    pub fn append(&mut self, parent: PathId, segment: PathSegment) -> Option<PathId> {
        let parent = self.local_id(parent)?;
        if let Some(path) = self.by_edge.get(&(ParentRef::Local(parent), segment)) {
            return Some(*path);
        }
        self.append_node(ParentRef::Local(parent), segment)
    }

    /// Create a validated link that can be used as a parent in another store.
    pub fn link(&self, parent: PathId) -> Option<PathLink> {
        let parent = self.local_id(parent)?;
        Some(PathLink {
            path: parent,
            depth: self.node(parent)?.depth,
        })
    }

    /// Append a child whose parent is owned by this or another path store.
    /// Parent depth is derived from the opaque link, so callers cannot supply
    /// inconsistent metadata.
    pub fn append_linked(&mut self, parent: PathLink, segment: PathSegment) -> Option<PathId> {
        if let Some(path) = self.by_edge.get(&(ParentRef::Linked(parent), segment)) {
            return Some(*path);
        }
        self.append_node(ParentRef::Linked(parent), segment)
    }

    fn append_node(&mut self, parent: ParentRef, segment: PathSegment) -> Option<PathId> {
        if self.nodes.len() >= self.max_nodes {
            return None;
        }
        let id = u32::try_from(self.nodes.len()).ok()?;
        let depth = parent_depth(&self.nodes, parent)?;
        let child = PathId::for_store(id, self.owner);
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment),
        });
        self.by_edge.insert((parent, segment), child);
        Some(child)
    }

    pub fn depth(&self, id: PathId) -> Option<u32> {
        self.node(id).map(|node| node.depth)
    }

    pub fn parent_ref(&self, id: PathId) -> Option<ParentRef> {
        self.node(id).map(|node| node.parent)
    }

    pub fn parent(&self, id: PathId) -> Option<PathId> {
        self.parent_ref(id).map(|parent| match parent {
            ParentRef::Local(id) => id,
            ParentRef::Linked(link) => link.path(),
        })
    }

    pub fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
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
            match self.parent_ref(current) {
                Some(ParentRef::Local(next)) => current = next,
                Some(ParentRef::Linked(_)) | None => return false,
            }
        }
        current == self.local_id(prefix).unwrap_or(PathId::EMPTY)
    }

    pub fn segment(&self, id: PathId) -> Option<&PathSegment> {
        let id = self.local_id(id)?;
        if id.is_empty() {
            return None;
        }
        self.nodes.get(id.index() as usize)?.segment.as_ref()
    }

    pub fn first_segment_of(&self, id: PathId) -> Option<&PathSegment> {
        let mut current = self.local_id(id)?;
        let mut last = None;
        while !current.is_empty() {
            let node = self.node(current)?;
            last = Some(self.segment(current)?);
            current = match node.parent {
                ParentRef::Local(parent) => parent,
                ParentRef::Linked(_) => return None,
            };
        }
        last
    }

    pub fn find_edge(&self, parent: PathId, segment: &PathSegment) -> Option<PathId> {
        let parent = self.local_id(parent)?;
        self.by_edge
            .get(&(ParentRef::Local(parent), *segment))
            .copied()
    }

    pub fn find_linked_edge(&self, parent: PathLink, segment: &PathSegment) -> Option<PathId> {
        self.by_edge
            .get(&(ParentRef::Linked(parent), *segment))
            .copied()
    }

    pub fn collect_segments(&self, id: PathId, buf: &mut Vec<PathSegment>) -> Option<()> {
        buf.clear();
        let mut current = self.local_id(id)?;
        while !current.is_empty() {
            let node = self.node(current)?;
            buf.push(*self.segment(current)?);
            current = match node.parent {
                ParentRef::Local(parent) => parent,
                ParentRef::Linked(_) => return None,
            };
        }
        buf.reverse();
        Some(())
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn max_nodes(&self) -> usize {
        self.max_nodes
    }

    pub fn last(&self, id: PathId) -> Option<&PathSegment> {
        self.segment(id)
    }

    pub fn first_index(&self, id: PathId) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    pub fn without_first(&self, id: PathId) -> Option<PathId> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    pub fn concat_with_buffer(
        &mut self,
        prefix: PathId,
        suffix: PathId,
        buf: &mut Vec<PathSegment>,
    ) -> Option<PathId> {
        self.collect_segments(suffix, buf)?;
        let mut result = self.local_id(prefix)?;
        for segment in buf.drain(..) {
            result = self.append(result, segment)?;
        }
        Some(result)
    }

    pub fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let mut buf = Vec::new();
        self.concat_with_buffer(prefix, suffix, &mut buf)
    }

    fn rebuild_without_first(&self, id: PathId) -> Option<PathId> {
        let mut segments = Vec::new();
        let mut current = self.local_id(id)?;
        loop {
            let node = self.node(current)?;
            if match node.parent {
                ParentRef::Local(parent) => parent.is_empty(),
                ParentRef::Linked(link) => link.path().is_empty(),
            } {
                break;
            }
            segments.push(*self.segment(current)?);
            current = match node.parent {
                ParentRef::Local(parent) => parent,
                ParentRef::Linked(_) => return None,
            };
        }
        let mut result = PathId::EMPTY;
        for seg in segments.into_iter().rev() {
            result = self.find_edge(result, &seg)?;
        }
        Some(result)
    }

    pub fn segments(&self, id: PathId) -> Option<PathSegments> {
        let mut collected = Vec::new();
        self.collect_segments(id, &mut collected)?;
        Some(PathSegments {
            segments: collected,
            index: 0,
        })
    }
}

fn parent_depth(nodes: &[PathNode], parent: ParentRef) -> Option<u32> {
    match parent {
        ParentRef::Local(id) => nodes.get(id.index() as usize)?.depth.checked_add(1),
        ParentRef::Linked(link) => link.depth.checked_add(1),
    }
}

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
        Some(*result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.segments.len().saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PathSegments {}

impl Default for PathStore {
    fn default() -> Self {
        Self::new()
    }
}
