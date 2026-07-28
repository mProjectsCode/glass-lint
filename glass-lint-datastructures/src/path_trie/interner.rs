use crate::{
    ParentPathStore, PathId, PathSegment, PathSegments, path_trie::DEFAULT_MAX_PATH_NODES,
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
        self.store.append(parent.0, segment).map(PathId)
    }

    pub fn depth(&self, path: PathId) -> Option<u32> {
        self.store.depth(path.0)
    }

    pub fn starts_with(&self, path: PathId, prefix: PathId) -> bool {
        self.store.starts_with(path.0, prefix.0)
    }

    pub fn store(&self) -> &ParentPathStore {
        &self.store
    }

    pub fn last(&self, path: PathId) -> Option<&PathSegment> {
        self.store.last(path.0)
    }

    pub fn first_index(&self, path: PathId) -> Option<u32> {
        self.store.first_index(path.0)
    }

    pub fn without_first(&self, path: PathId) -> Option<PathId> {
        self.store.without_first(path.0).map(PathId)
    }

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

    pub fn concat(&mut self, prefix: PathId, suffix: PathId) -> Option<PathId> {
        let mut buf = Vec::new();
        self.concat_with_buffer(prefix, suffix, &mut buf)
    }

    pub fn segments(&self, path: PathId) -> PathSegments {
        self.store.segments(path.0)
    }

    pub fn node_count(&self) -> usize {
        self.store.node_count()
    }

    pub fn checked_id(&self, raw: u32) -> Option<PathId> {
        if self.store.is_valid(raw) {
            Some(PathId(raw))
        } else {
            None
        }
    }
}

impl Default for PathInterner {
    fn default() -> Self {
        Self::new()
    }
}
