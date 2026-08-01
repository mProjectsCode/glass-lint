use hashbrown::HashMap;

use crate::{PathId, PathSegment};

#[derive(Debug, Clone)]
struct PathNode {
    parent: PathId,
    depth: u32,
    segment: Option<PathSegment>,
}

#[derive(Debug)]
pub struct ParentPathStore {
    nodes: Vec<PathNode>,
    by_edge: HashMap<(PathId, PathSegment), PathId>,
    max_nodes: usize,
}

impl ParentPathStore {
    pub fn new(max_nodes: usize) -> Self {
        Self {
            nodes: vec![PathNode {
                parent: PathId::EMPTY,
                depth: 0,
                segment: None,
            }],
            by_edge: HashMap::new(),
            max_nodes,
        }
    }

    /// Construct a `PathId` from a raw `u32` value, validating that the
    /// untagged index is within the node store. Returns `None` when the
    /// index is out of range.
    pub fn checked_id(&self, raw: u32) -> Option<PathId> {
        let idx = PathId(raw).untag().0 as usize;
        if idx < self.nodes.len() {
            Some(PathId(raw))
        } else {
            None
        }
    }

    pub fn is_valid(&self, id: PathId) -> bool {
        let idx = id.untag().0 as usize;
        idx < self.nodes.len()
    }

    pub fn append(&mut self, parent: PathId, segment: PathSegment) -> Option<PathId> {
        if !self.is_valid(parent) {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment)) {
            return Some(*path);
        }
        if self.nodes.len() >= self.max_nodes {
            return None;
        }
        let id = u32::try_from(self.nodes.len()).ok()?;
        let depth = self.nodes[parent.untag().0 as usize].depth.checked_add(1)?;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment),
        });
        self.by_edge.insert((parent, segment), PathId(id));
        Some(PathId(id))
    }

    /// Append a child whose parent may be owned by another path store.
    ///
    /// The caller supplies the validated depth of the parent because an
    /// external parent is intentionally not looked up in this store. The
    /// child depth is computed here, and the operation remains deduplicated
    /// and capacity-bounded like [`Self::append`].
    pub fn append_linked(
        &mut self,
        parent: PathId,
        segment: PathSegment,
        parent_depth: u32,
    ) -> Option<PathId> {
        if let Some(path) = self.by_edge.get(&(parent, segment)) {
            return Some(*path);
        }
        if self.nodes.len() >= self.max_nodes {
            return None;
        }
        let id = u32::try_from(self.nodes.len()).ok()?;
        let depth = parent_depth.checked_add(1)?;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment),
        });
        self.by_edge.insert((parent, segment), PathId(id));
        Some(PathId(id))
    }

    pub fn depth(&self, id: PathId) -> Option<u32> {
        let idx = id.untag().0 as usize;
        self.nodes.get(idx).map(|node| node.depth)
    }

    pub fn parent(&self, id: PathId) -> Option<PathId> {
        let idx = id.untag().0 as usize;
        self.nodes.get(idx).map(|node| node.parent)
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
            match self.parent(current) {
                Some(next) => current = next,
                None => return false,
            }
        }
        current == prefix
    }

    pub fn segment(&self, id: PathId) -> Option<&PathSegment> {
        let idx = id.untag().0 as usize;
        if idx == 0 {
            return None;
        }
        self.nodes.get(idx)?.segment.as_ref()
    }

    pub fn first_segment_of(&self, id: PathId) -> Option<&PathSegment> {
        let mut current = id;
        let mut last = None;
        while !current.is_empty() {
            let idx = current.untag().0 as usize;
            let node = self.nodes.get(idx)?;
            last = Some(self.segment(current)?);
            current = node.parent;
        }
        last
    }

    pub fn find_edge(&self, parent: PathId, segment: &PathSegment) -> Option<PathId> {
        self.by_edge.get(&(parent, *segment)).copied()
    }

    pub fn collect_segments(&self, id: PathId, buf: &mut Vec<PathSegment>) -> Option<()> {
        buf.clear();
        let mut current = id;
        while !current.is_empty() {
            let idx = current.untag().0 as usize;
            let node = self.nodes.get(idx)?;
            buf.push(*self.segment(current)?);
            current = node.parent;
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

    fn rebuild_without_first(&self, id: PathId) -> Option<PathId> {
        let mut segments = Vec::new();
        let mut current = id;
        loop {
            let idx = current.untag().0 as usize;
            let node = self.nodes.get(idx)?;
            if node.parent == PathId::EMPTY {
                break;
            }
            segments.push(*self.segment(current)?);
            current = node.parent;
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
