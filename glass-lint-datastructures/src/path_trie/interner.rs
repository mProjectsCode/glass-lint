use crate::{
    ParentPathStore, ParentRef, PathId, PathLink, PathSegment, PathSegments,
    path_trie::DEFAULT_MAX_PATH_NODES,
};

#[derive(Debug)]
pub struct PathInterner {
    store: ParentPathStore,
}

impl PathInterner {
    pub fn new() -> Self {
        Self {
            store: ParentPathStore::new(DEFAULT_MAX_PATH_NODES),
        }
    }

    pub fn append(&mut self, parent: PathId, segment: PathSegment) -> Option<PathId> {
        self.store.append(parent, segment)
    }

    pub fn depth(&self, path: PathId) -> Option<u32> {
        self.store.depth(path)
    }

    pub fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
        self.store.starts_with(path, prefix)
    }

    pub fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.store.last(path)
    }

    pub fn segment(&self, path: PathId) -> Option<&PathSegment> {
        self.store.segment(path)
    }

    pub fn first_segment_of(&self, path: PathId) -> Option<&PathSegment> {
        self.store.first_segment_of(path)
    }

    pub fn is_valid(&self, path: PathId) -> bool {
        self.store.is_valid(path)
    }

    pub fn parent_ref(&self, path: PathId) -> Option<ParentRef> {
        self.store.parent_ref(path)
    }

    pub fn parent(&self, path: PathId) -> Option<PathId> {
        self.store.parent(path)
    }

    pub fn collect_segments(&self, path: PathId, buf: &mut Vec<PathSegment>) -> Option<()> {
        self.store.collect_segments(path, buf)
    }

    pub fn link(&self, path: PathId) -> Option<PathLink> {
        self.store.link(path)
    }

    pub fn find_edge(&self, parent: PathId, segment: &PathSegment) -> Option<PathId> {
        self.store.find_edge(parent, segment)
    }

    pub fn first_index(&self, path: PathId) -> Option<u32> {
        self.store.first_index(path)
    }

    pub fn without_first(&self, path: PathId) -> Option<PathId> {
        self.store.without_first(path)
    }

    pub fn concat_with_buffer(
        &mut self,
        prefix: PathId,
        suffix: PathId,
        buf: &mut Vec<PathSegment>,
    ) -> Option<PathId> {
        self.store.collect_segments(suffix, buf)?;
        let mut result = prefix;
        for segment in buf.drain(..) {
            result = self.append(result, segment)?;
        }
        Some(result)
    }

    pub fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let mut buf = Vec::new();
        self.concat_with_buffer(prefix, suffix, &mut buf)
    }

    pub fn segments(&self, path: PathId) -> Option<PathSegments> {
        self.store.segments(path)
    }

    pub fn node_count(&self) -> usize {
        self.store.node_count()
    }

    pub fn checked_id(&self, raw: u32) -> Option<PathId> {
        self.store.checked_id(raw)
    }
}

impl Default for PathInterner {
    fn default() -> Self {
        Self::new()
    }
}
