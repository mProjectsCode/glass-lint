use std::collections::{BTreeMap, VecDeque};

use swc_common::BytePos;

use crate::analysis::scope::{ScopeId, ScopeKind};

#[derive(Debug, Clone, Copy)]
pub struct ScopeShape {
    scope_id: ScopeId,
    kind: ScopeKind,
    span: swc_common::Span,
    parent: Option<ScopeId>,
}

impl ScopeShape {
    pub(crate) fn new(
        scope_id: ScopeId,
        kind: ScopeKind,
        span: swc_common::Span,
        parent: Option<ScopeId>,
    ) -> Self {
        Self {
            scope_id,
            kind,
            span,
            parent,
        }
    }

    pub(crate) fn scope_id(self) -> ScopeId {
        self.scope_id
    }

    pub(crate) fn kind(self) -> ScopeKind {
        self.kind
    }

    pub(crate) fn span(self) -> swc_common::Span {
        self.span
    }

    pub(crate) fn parent(self) -> Option<ScopeId> {
        self.parent
    }
}

#[derive(Debug, Default)]
pub struct ScopeShapeTable {
    #[cfg(test)]
    recorded: usize,
    children: BTreeMap<ScopeShapeKey, VecDeque<ScopeId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopeShapeKey {
    parent: Option<ScopeId>,
    span_lo: BytePos,
    kind: ScopeKind,
}

impl ScopeShapeTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&mut self, shape: ScopeShape) {
        let key = ScopeShapeKey {
            parent: shape.parent(),
            span_lo: shape.span().lo,
            kind: shape.kind(),
        };
        #[cfg(test)]
        {
            self.recorded = self.recorded.saturating_add(1);
        }
        self.children
            .entry(key)
            .or_default()
            .push_back(shape.scope_id());
    }

    pub(crate) fn take_child(
        &mut self,
        parent: Option<ScopeId>,
        span_lo: BytePos,
        kind: ScopeKind,
    ) -> Option<ScopeId> {
        self.children
            .get_mut(&ScopeShapeKey {
                parent,
                span_lo,
                kind,
            })
            .and_then(VecDeque::pop_front)
    }

    #[cfg(test)]
    pub(crate) fn shapes_len(&self) -> usize {
        self.recorded
    }

    #[cfg(test)]
    pub(crate) fn remaining(
        &self,
        parent: Option<ScopeId>,
        span_lo: BytePos,
        kind: ScopeKind,
    ) -> usize {
        self.children
            .get(&ScopeShapeKey {
                parent,
                span_lo,
                kind,
            })
            .map_or(0, VecDeque::len)
    }

    pub(crate) fn is_consumed(&self) -> bool {
        self.children.values().all(VecDeque::is_empty)
    }
}
