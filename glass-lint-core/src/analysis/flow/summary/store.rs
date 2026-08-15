use glass_lint_datastructures::{ParentRef, PathId, PathSegment, PathStore};

const MAX_OVERLAY_NODES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis::flow) enum SummaryPathId {
    Frozen(PathId),
    Overlay(PathId),
}

impl SummaryPathId {
    pub(super) const EMPTY: Self = Self::Frozen(PathId::EMPTY);

    pub(super) fn is_empty(self) -> bool {
        self.path_id().is_empty()
    }

    #[cfg(test)]
    pub(super) fn is_frozen(self) -> bool {
        matches!(self, Self::Frozen(_))
    }

    pub(super) fn from_frozen_path(id: PathId) -> Self {
        Self::Frozen(id)
    }

    fn from_overlay_path(id: PathId) -> Self {
        Self::Overlay(id)
    }

    fn path_id(self) -> PathId {
        match self {
            Self::Frozen(id) | Self::Overlay(id) => id,
        }
    }
}

#[derive(Debug)]
pub(in crate::analysis::flow) struct SummaryPathStore<'a> {
    frozen: &'a PathStore,
    overlay: PathStore,
}

/// Walk one summary path from its leaf back to the empty root.
///
/// The frozen/overlay representation boundary is handled by the store's
/// `parent` and `segment` operations; callers consume one representation-
/// neutral segment sequence.
struct SummaryPathWalk<'a> {
    store: &'a SummaryPathStore<'a>,
    current: SummaryPathId,
}

impl<'a> SummaryPathWalk<'a> {
    fn new(store: &'a SummaryPathStore<'a>, id: SummaryPathId) -> Option<Self> {
        if !id.is_empty() {
            store.segment(id)?;
        }
        Some(Self { store, current: id })
    }

    #[cfg(test)]
    fn segments(self) -> Option<Vec<PathSegment>> {
        let mut segments = Vec::new();
        self.visit(&mut |segment| segments.push(*segment))?;
        Some(segments)
    }

    fn visit(self, visit: &mut impl FnMut(&PathSegment)) -> Option<()> {
        if self.current.is_empty() {
            return Some(());
        }
        let segment = *self.store.segment(self.current)?;
        let parent = self.store.parent(self.current)?;
        Self {
            store: self.store,
            current: parent,
        }
        .visit(visit)?;
        visit(&segment);
        Some(())
    }

    fn reaches(mut self, prefix: SummaryPathId, distance: u32) -> bool {
        for _ in 0..distance {
            let Some(parent) = self.store.parent(self.current) else {
                return false;
            };
            self.current = parent;
        }
        self.current == prefix
    }
}

impl<'a> SummaryPathStore<'a> {
    pub(super) fn new(frozen: &'a PathStore) -> Self {
        Self {
            frozen,
            overlay: PathStore::with_max_nodes(MAX_OVERLAY_NODES),
        }
    }

    pub(super) fn is_valid(&self, id: SummaryPathId) -> bool {
        match id {
            SummaryPathId::Frozen(path) => self.frozen.is_valid(path),
            SummaryPathId::Overlay(path) => self.overlay.is_valid(path),
        }
    }

    pub(super) fn intern_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        self.frozen
            .is_valid(path)
            .then_some(SummaryPathId::Frozen(path))
    }

    pub(super) fn depth(&self, id: SummaryPathId) -> Option<u32> {
        match id {
            SummaryPathId::Frozen(path) => self.frozen.depth(path),
            SummaryPathId::Overlay(path) => self.overlay.depth(path),
        }
    }

    fn parent(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        match id {
            SummaryPathId::Frozen(path) => match self.frozen.parent_ref(path)? {
                ParentRef::Local(parent) => Some(SummaryPathId::Frozen(parent)),
                ParentRef::Linked(_) => None,
            },
            SummaryPathId::Overlay(path) => match self.overlay.parent_ref(path)? {
                ParentRef::Local(parent) => Some(SummaryPathId::Overlay(parent)),
                ParentRef::Linked(link) => Some(SummaryPathId::Frozen(link.path())),
            },
        }
    }

    pub(super) fn starts_with(&self, id: SummaryPathId, prefix: SummaryPathId) -> bool {
        let Some(path_depth) = self.depth(id) else {
            return false;
        };
        let Some(prefix_depth) = self.depth(prefix) else {
            return false;
        };
        prefix_depth <= path_depth
            && SummaryPathWalk::new(self, id)
                .is_some_and(|walk| walk.reaches(prefix, path_depth - prefix_depth))
    }

    pub(in crate::analysis::flow) fn matches_frozen(
        &self,
        id: SummaryPathId,
        base: PathId,
    ) -> bool {
        let Some(base) = self.intern_frozen(base) else {
            return false;
        };
        self.is_valid(id) && id == base
    }

    pub(crate) fn starts_with_frozen(&self, id: SummaryPathId, prefix: PathId) -> bool {
        let Some(prefix_id) = self.intern_frozen(prefix) else {
            return false;
        };
        self.starts_with(id, prefix_id)
    }

    fn segment(&self, id: SummaryPathId) -> Option<&PathSegment> {
        match id {
            SummaryPathId::Frozen(path) => self.frozen.segment(path),
            SummaryPathId::Overlay(path) => self.overlay.segment(path),
        }
    }

    fn first_segment_of(&self, id: SummaryPathId) -> Option<&PathSegment> {
        match id {
            SummaryPathId::Frozen(path) => self.frozen.first_segment_of(path),
            SummaryPathId::Overlay(path) => self.overlay.first_segment_of(path),
        }
    }

    pub(super) fn first_index(&self, id: SummaryPathId) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    fn find_edge_impl(&self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        match parent {
            SummaryPathId::Overlay(path) => self
                .overlay
                .find_edge(path, &segment)
                .map(SummaryPathId::from_overlay_path),
            SummaryPathId::Frozen(path) => self
                .frozen
                .find_edge(path, &segment)
                .map(SummaryPathId::from_frozen_path),
        }
    }

    fn find_edge(&self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        self.find_edge_impl(parent, segment)
    }

    fn overlay_append(
        &mut self,
        parent: SummaryPathId,
        segment: PathSegment,
    ) -> Option<SummaryPathId> {
        if let SummaryPathId::Overlay(path) = parent {
            return self
                .overlay
                .append(path, segment)
                .map(SummaryPathId::from_overlay_path);
        }
        let parent_link = self.frozen.link(parent.path_id())?;
        if let Some(child) = self.overlay.find_linked_edge(parent_link, &segment) {
            return Some(SummaryPathId::from_overlay_path(child));
        }
        let child = self.overlay.append_linked(parent_link, segment)?;
        Some(SummaryPathId::from_overlay_path(child))
    }

    fn append(&mut self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        if let Some(child) = self.find_edge(parent, segment) {
            return Some(child);
        }
        self.overlay_append(parent, segment)
    }

    pub(super) fn join(
        &mut self,
        prefix: SummaryPathId,
        suffix: SummaryPathId,
    ) -> Option<SummaryPathId> {
        if suffix.is_empty() {
            return Some(prefix);
        }
        self.join_suffix(prefix, suffix)
    }

    pub(super) fn without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        self.segment(id)?;
        let depth = self.depth(id)?;
        self.without_first_from(id, depth)
    }

    #[cfg(test)]
    pub(super) fn owned_segments(&self, id: SummaryPathId) -> Option<Vec<PathSegment>> {
        SummaryPathWalk::new(self, id)?.segments()
    }

    pub(super) fn visit_segments(
        &self,
        id: SummaryPathId,
        visit: &mut impl FnMut(&PathSegment),
    ) -> Option<()> {
        if id.is_empty() {
            return Some(());
        }
        SummaryPathWalk::new(self, id)?.visit(visit)
    }

    fn join_suffix(
        &mut self,
        prefix: SummaryPathId,
        suffix: SummaryPathId,
    ) -> Option<SummaryPathId> {
        if suffix.is_empty() {
            return Some(prefix);
        }
        let parent = self.parent(suffix)?;
        let prefix = self.join_suffix(prefix, parent)?;
        let segment = *self.segment(suffix)?;
        self.append(prefix, segment)
    }

    fn without_first_from(&self, current: SummaryPathId, depth: u32) -> Option<SummaryPathId> {
        if current.is_empty() {
            return Some(SummaryPathId::EMPTY);
        }
        let parent = self.parent(current)?;
        let result = self.without_first_from(parent, depth.saturating_sub(1))?;
        if depth == 1 {
            return Some(result);
        }
        self.find_edge(result, *self.segment(current)?)
    }

    #[cfg(test)]
    pub(super) fn with_max_nodes(frozen: &'a PathStore, max_nodes: usize) -> Self {
        Self {
            frozen,
            overlay: PathStore::with_max_nodes(max_nodes),
        }
    }
}

#[cfg(test)]
mod tests;
