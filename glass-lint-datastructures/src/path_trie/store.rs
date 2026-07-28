use hashbrown::HashMap;

use crate::{PathId, PathSegment};

#[derive(Debug, Clone)]
pub struct PathNode {
    parent: u32,
    depth: u32,
    segment: Option<PathSegment>,
}

impl PathNode {
    pub fn parent(&self) -> u32 {
        self.parent
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn segment(&self) -> Option<&PathSegment> {
        self.segment.as_ref()
    }
}

#[derive(Debug)]
pub struct ParentPathStore {
    nodes: Vec<PathNode>,
    by_edge: HashMap<(u32, PathSegment), u32>,
    max_nodes: usize,
}

impl ParentPathStore {
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

    pub fn is_valid(&self, id: u32) -> bool {
        let idx = id as usize;
        idx < self.nodes.len()
    }

    pub fn append(&mut self, parent: u32, segment: PathSegment) -> Option<u32> {
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
        let depth = self.nodes[parent as usize].depth.checked_add(1)?;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    pub fn append_linked(&mut self, parent: u32, segment: PathSegment, depth: u32) -> Option<u32> {
        if self.node_count() >= self.max_nodes {
            return None;
        }
        if let Some(path) = self.by_edge.get(&(parent, segment)) {
            return Some(*path);
        }
        let id = u32::try_from(self.nodes.len()).ok()? | PathId::LINK_TAG;
        self.nodes.push(PathNode {
            parent,
            depth,
            segment: Some(segment),
        });
        self.by_edge.insert((parent, segment), id);
        Some(id)
    }

    pub fn depth(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.depth)
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let id = PathId(id).untag().0 as usize;
        self.nodes.get(id).map(|node| node.parent)
    }

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

    pub fn segment(&self, id: u32) -> Option<&PathSegment> {
        let id = PathId(id).untag().0 as usize;
        if id == 0 {
            return None;
        }
        self.nodes.get(id)?.segment.as_ref()
    }

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

    pub fn find_edge(&self, parent: u32, segment: &PathSegment) -> Option<u32> {
        self.by_edge.get(&(parent, *segment)).copied()
    }

    pub fn find_linked_edge(&self, parent: u32, segment: &PathSegment) -> Option<u32> {
        self.find_edge(parent, segment)
    }

    pub fn collect_segments(&self, id: u32, buf: &mut Vec<PathSegment>) -> Option<()> {
        buf.clear();
        let mut current = id;
        while current != 0 {
            let node = self.nodes.get(current as usize)?;
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

    pub fn last(&self, id: u32) -> Option<&PathSegment> {
        self.segment(id)
    }

    pub fn first_index(&self, id: u32) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    pub fn without_first(&self, id: u32) -> Option<u32> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    fn rebuild_without_first(&self, id: u32) -> Option<u32> {
        let mut segments = Vec::new();
        let mut current = id;
        loop {
            let node = self.nodes.get(current as usize)?;
            if node.parent == 0 {
                break;
            }
            segments.push(*self.segment(current)?);
            current = node.parent;
        }
        let mut result = 0;
        for seg in segments.into_iter().rev() {
            result = self.find_edge(result, &seg)?;
        }
        Some(result)
    }

    pub fn segments(&self, id: u32) -> PathSegments {
        let mut collected = Vec::new();
        self.collect_segments(id, &mut collected);
        PathSegments {
            segments: collected,
            index: 0,
        }
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
