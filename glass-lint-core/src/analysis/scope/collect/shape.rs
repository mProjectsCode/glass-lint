use std::collections::{BTreeMap, VecDeque};

use swc_common::BytePos;

use crate::analysis::scope::{ScopeId, ScopeKind};

#[derive(Debug, Clone, Copy)]
pub struct ScopeShape {
    pub(crate) scope_id: ScopeId,
    pub(crate) kind: ScopeKind,
    pub(crate) span: swc_common::Span,
    pub(crate) parent: Option<ScopeId>,
}

#[derive(Debug, Default)]
pub struct ScopeShapeTable {
    pub(crate) shapes: Vec<ScopeShape>,
    pub(crate) children: BTreeMap<ScopeShapeKey, VecDeque<ScopeId>>,
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
            parent: shape.parent,
            span_lo: shape.span.lo,
            kind: shape.kind,
        };
        self.shapes.push(shape);
        self.children
            .entry(key)
            .or_default()
            .push_back(shape.scope_id);
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
        self.shapes.len()
    }

    #[cfg(test)]
    pub(crate) fn remaining(&self, parent: Option<ScopeId>, span_lo: BytePos, kind: ScopeKind) -> usize {
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
