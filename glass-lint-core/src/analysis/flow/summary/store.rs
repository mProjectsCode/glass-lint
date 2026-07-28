use glass_lint_datastructures::{ParentPathStore, PathId, PathInterner, PathSegment};

const MAX_OVERLAY_NODES: usize = 4096;
const OVERLAY_TAG: u32 = 1 << 31;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SummaryPathId(u32);

impl SummaryPathId {
    pub(super) const EMPTY: Self = Self(0);

    pub(super) fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    pub(super) fn is_frozen(self) -> bool {
        self.0 & OVERLAY_TAG == 0
    }

    pub(super) fn from_path_id(id: PathId) -> Self {
        Self(id.as_u32())
    }
}

#[derive(Debug)]
pub struct SummaryPathStore<'a> {
    frozen: &'a PathInterner,
    overlay: ParentPathStore,
}

impl<'a> SummaryPathStore<'a> {
    pub(super) fn new(frozen: &'a PathInterner) -> Self {
        Self {
            frozen,
            overlay: ParentPathStore::new(MAX_OVERLAY_NODES),
        }
    }

    pub(super) fn is_valid(&self, id: SummaryPathId) -> bool {
        if id.is_frozen() {
            self.frozen.store().is_valid(id.0)
        } else {
            self.overlay.is_valid(id.0 & !OVERLAY_TAG)
        }
    }

    pub(super) fn intern_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        if !self.frozen.store().is_valid(path.as_u32()) {
            return None;
        }
        Some(SummaryPathId::from_path_id(path))
    }

    pub(super) fn resolve_frozen(&self, path: PathId) -> Option<SummaryPathId> {
        if !self.frozen.store().is_valid(path.as_u32()) {
            return None;
        }
        Some(SummaryPathId::from_path_id(path))
    }

    fn depth_impl(&self, id: u32, is_frozen: bool) -> Option<u32> {
        if is_frozen {
            self.frozen.store().depth(id)
        } else {
            self.overlay.depth(id & !OVERLAY_TAG)
        }
    }

    pub(super) fn depth(&self, id: SummaryPathId) -> Option<u32> {
        self.depth_impl(id.0, id.is_frozen())
    }

    fn parent_impl(&self, id: u32, is_frozen: bool) -> Option<u32> {
        if is_frozen {
            self.frozen.store().parent(id)
        } else {
            self.overlay.parent(id & !OVERLAY_TAG)
        }
    }

    fn parent(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        let raw = self.parent_impl(id.0, id.is_frozen())?;
        Some(SummaryPathId(raw))
    }

    pub(super) fn starts_with(&self, id: SummaryPathId, prefix: SummaryPathId) -> bool {
        let Some(path_depth) = self.depth_impl(id.0, id.is_frozen()) else {
            return false;
        };
        let Some(prefix_depth) = self.depth_impl(prefix.0, prefix.is_frozen()) else {
            return false;
        };
        if prefix_depth > path_depth {
            return false;
        }
        let mut current = id;
        for _ in 0..(path_depth - prefix_depth) {
            match self.parent(current) {
                Some(next) => current = next,
                None => return false,
            }
        }
        current == prefix
    }

    pub(crate) fn matches_frozen(id: SummaryPathId, base: PathId) -> bool {
        id == SummaryPathId::from_path_id(base)
    }

    pub(crate) fn starts_with_frozen(&self, id: SummaryPathId, prefix: PathId) -> bool {
        let prefix_id = SummaryPathId::from_path_id(prefix);
        if !self.is_valid(prefix_id) {
            return false;
        }
        self.starts_with(id, prefix_id)
    }

    fn segment_impl(&self, raw_id: u32) -> Option<&PathSegment> {
        if raw_id == 0 || raw_id & OVERLAY_TAG == 0 {
            self.frozen.store().segment(raw_id)
        } else {
            self.overlay.segment(raw_id & !OVERLAY_TAG)
        }
    }

    fn segment(&self, id: SummaryPathId) -> Option<&PathSegment> {
        self.segment_impl(id.0)
    }

    fn first_segment_of_impl(&self, raw_id: u32) -> Option<&PathSegment> {
        if raw_id == 0 || raw_id & OVERLAY_TAG == 0 {
            self.frozen.store().first_segment_of(raw_id)
        } else {
            self.overlay.first_segment_of(raw_id & !OVERLAY_TAG)
        }
    }

    fn first_segment_of(&self, id: SummaryPathId) -> Option<&PathSegment> {
        self.first_segment_of_impl(id.0)
    }

    pub(super) fn first_index(&self, id: SummaryPathId) -> Option<u32> {
        match self.first_segment_of(id)? {
            PathSegment::Index(index) => Some(*index),
            PathSegment::Property(_) => None,
        }
    }

    fn find_edge_impl(&self, parent: u32, segment: PathSegment) -> Option<u32> {
        if let Some(child) = self.overlay.find_linked_edge(parent, &segment) {
            return Some(child);
        }
        if parent & OVERLAY_TAG == 0
            && let Some(child) = self.frozen.store().find_edge(parent, &segment)
        {
            return Some(child);
        }
        None
    }

    fn find_edge(&self, parent: SummaryPathId, segment: PathSegment) -> Option<SummaryPathId> {
        self.find_edge_impl(parent.0, segment).map(SummaryPathId)
    }

    fn overlay_append(
        &mut self,
        parent: SummaryPathId,
        segment: PathSegment,
    ) -> Option<SummaryPathId> {
        if self.overlay.node_count() >= self.overlay.max_nodes() {
            return None;
        }
        let depth = self.depth(parent)?.checked_add(1)?;
        self.overlay
            .append_linked(parent.0, segment, depth)
            .map(SummaryPathId)
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
        let mut segments = Vec::new();
        let mut current = suffix;
        while !current.is_empty() {
            segments.push(*self.segment_impl(current.0)?);
            current = SummaryPathId(self.parent_impl(current.0, current.is_frozen())?);
        }
        let mut result = prefix;
        for seg in segments.into_iter().rev() {
            result = self.append(result, seg)?;
        }
        Some(result)
    }

    pub(super) fn without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        self.segment(id)?;
        self.rebuild_without_first(id)
    }

    fn rebuild_without_first(&self, id: SummaryPathId) -> Option<SummaryPathId> {
        let mut segments = Vec::new();
        let mut current = id;
        loop {
            let node_parent = self.parent_impl(current.0, current.is_frozen())?;
            if node_parent == 0 {
                break;
            }
            segments.push(*self.segment_impl(current.0)?);
            current = SummaryPathId(node_parent);
        }
        let mut result = SummaryPathId::EMPTY;
        for seg in segments.into_iter().rev() {
            result = self.find_edge(result, seg)?;
        }
        Some(result)
    }

    #[cfg(test)]
    pub(super) fn owned_segments(&self, id: SummaryPathId) -> Option<Vec<PathSegment>> {
        let depth = self.depth(id)?;
        let mut segments = Vec::with_capacity(depth as usize);
        let mut current = id;
        while !current.is_empty() {
            segments.push(*self.segment_impl(current.0)?);
            let next_parent = self.parent_impl(current.0, current.is_frozen())?;
            current = SummaryPathId(next_parent);
        }
        segments.reverse();
        Some(segments)
    }

    pub(super) fn visit_segments(
        &self,
        id: SummaryPathId,
        visit: &mut impl FnMut(&PathSegment),
    ) -> Option<()> {
        if id.is_empty() {
            return Some(());
        }
        let mut segments = Vec::new();
        let mut current = id;
        while !current.is_empty() {
            segments.push(*self.segment(current)?);
            current = self.parent(current)?;
        }
        for seg in segments.into_iter().rev() {
            visit(&seg);
        }
        Some(())
    }

    #[cfg(test)]
    pub(super) fn with_max_nodes(frozen: &'a PathInterner, max_nodes: usize) -> Self {
        Self {
            frozen,
            overlay: ParentPathStore::new(max_nodes),
        }
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::{PathId, PathInterner, PathSegment};

    use super::*;

    fn make_frozen_paths() -> (PathInterner, PathId, PathId, PathId) {
        let mut frozen = PathInterner::new();
        let a = frozen.append(PathId::EMPTY, PathSegment::Index(0)).unwrap();
        let b = frozen.append(a, PathSegment::Index(1)).unwrap();
        let c = frozen.append(a, PathSegment::Index(2)).unwrap();
        (frozen, a, b, c)
    }

    #[test]
    fn frozen_path_is_referenced_without_copy() {
        let (frozen, a, _b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        let s_id = store.intern_frozen(a).unwrap();
        assert_eq!(s_id, SummaryPathId::from_path_id(a));
        assert!(s_id.is_frozen());
        assert_eq!(store.depth(s_id), Some(1));
    }

    #[test]
    fn invalid_frozen_path_returns_none() {
        let empty = PathInterner::new();
        let (frozen, a, _b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&empty);
        assert!(store.intern_frozen(a).is_none());
        assert!(store.resolve_frozen(a).is_none());
        // a is valid in `frozen` but not in `empty` — validates that
        // cross-store IDs are rejected
        assert!(frozen.checked_id(a.as_u32()).is_some());
        assert!(empty.checked_id(a.as_u32()).is_none());
    }

    #[test]
    fn join_frozen_prefix_with_frozen_suffix_creates_overlay_node() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let prefix = store.intern_frozen(a).unwrap();
        let suffix = store.intern_frozen(b).unwrap();
        let joined = store.join(prefix, suffix).unwrap();
        assert!(!joined.is_frozen());
        assert!(!joined.is_empty());
        assert_eq!(store.depth(joined), Some(3));
    }

    #[test]
    fn join_with_empty_is_identity() {
        let (frozen, a, _b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let prefix = store.intern_frozen(a).unwrap();
        assert_eq!(store.join(prefix, SummaryPathId::EMPTY), Some(prefix));
        assert_eq!(store.join(SummaryPathId::EMPTY, prefix), Some(prefix));
    }

    #[test]
    fn frozen_reference_reused_by_multiple_summaries() {
        let (frozen, a, _b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        let id1 = store.intern_frozen(a).unwrap();
        let id2 = store.intern_frozen(a).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn starts_with_mixed_frozen_and_overlay() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        let ab = store.join(a_s, b_s).unwrap();
        assert!(store.starts_with(ab, a_s));
        assert!(store.starts_with(ab, ab));
    }

    #[test]
    fn matches_frozen_checks_identity() {
        let (_, a, b, _c) = make_frozen_paths();
        assert!(SummaryPathStore::matches_frozen(
            SummaryPathId::from_path_id(a),
            a
        ));
        assert!(!SummaryPathStore::matches_frozen(
            SummaryPathId::from_path_id(a),
            b,
        ));
    }

    #[test]
    fn starts_with_frozen_checks_prefix() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        let ab = store.join(a_s, b_s).unwrap();
        assert!(store.starts_with_frozen(ab, a));
        assert!(!store.starts_with_frozen(a_s, b));
    }

    #[test]
    fn without_first_on_frozen() {
        let (frozen, _a, b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        let s_b = SummaryPathId::from_path_id(b);
        assert!(store.without_first(s_b).is_none());
    }

    #[test]
    fn without_first_on_overlay() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        let ab = store.join(a_s, b_s).unwrap();
        let result = store.without_first(ab).unwrap();
        assert_eq!(result, b_s);
    }

    #[test]
    fn owned_segments_on_frozen() {
        let (frozen, _a, b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        let s_b = SummaryPathId::from_path_id(b);
        let segs = store.owned_segments(s_b).unwrap();
        assert_eq!(segs, vec![PathSegment::Index(0), PathSegment::Index(1)]);
    }

    #[test]
    fn owned_segments_on_joined_overlay() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        let ab = store.join(a_s, b_s).unwrap();
        let segs = store.owned_segments(ab).unwrap();
        assert_eq!(
            segs,
            vec![
                PathSegment::Index(0),
                PathSegment::Index(0),
                PathSegment::Index(1),
            ]
        );
    }

    #[test]
    fn overlay_budget_exhaustion_fails_closed() {
        let (frozen, a, b, _c) = make_frozen_paths();
        let mut store = SummaryPathStore::with_max_nodes(&frozen, 2);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        assert!(store.join(a_s, b_s).is_none());
    }

    #[test]
    fn empty_summary_path_has_no_segments() {
        let (frozen, _a, _b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        assert_eq!(store.depth(SummaryPathId::EMPTY), Some(0));
        assert_eq!(store.first_index(SummaryPathId::EMPTY), None);
        assert_eq!(store.without_first(SummaryPathId::EMPTY), None);
    }

    #[test]
    fn first_index_on_frozen_and_overlay() {
        let (frozen, a, _b, _c) = make_frozen_paths();
        let store = SummaryPathStore::new(&frozen);
        let s_idx = SummaryPathId::from_path_id(a);
        assert_eq!(store.first_index(s_idx), Some(0));
    }

    #[test]
    fn join_order_with_three_segments() {
        let (frozen, a, b, c) = make_frozen_paths();
        let mut store = SummaryPathStore::new(&frozen);
        let a_s = store.intern_frozen(a).unwrap();
        let b_s = store.intern_frozen(b).unwrap();
        let c_s = store.intern_frozen(c).unwrap();
        let ab = store.join(a_s, b_s).unwrap();
        let abc = store.join(ab, c_s).unwrap();
        assert_eq!(store.depth(abc), Some(5));
        assert!(store.starts_with(abc, a_s));
        let segs = store.owned_segments(abc).unwrap();
        assert_eq!(
            segs,
            vec![
                PathSegment::Index(0),
                PathSegment::Index(0),
                PathSegment::Index(1),
                PathSegment::Index(0),
                PathSegment::Index(2),
            ]
        );
    }
}
