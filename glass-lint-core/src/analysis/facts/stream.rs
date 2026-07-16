//! Immutable semantic fact stream with deterministic insertion order.

use std::cell::RefCell;

#[cfg(test)]
use swc_common::BytePos;

#[cfg(test)]
use super::FactKind;
use super::{FactId, MAX_FACTS, SemanticFact};
use crate::analysis::value::{PathId, PathInterner, PathSegment};

#[derive(Debug)]
pub(in crate::analysis) struct FactStream {
    facts: Vec<SemanticFact>,
    paths: RefCell<PathInterner>,
    valid: bool,
}

impl FactStream {
    /// Create an empty, valid stream. Fact IDs are assigned by the builder;
    /// this type verifies the resulting sequence as facts are appended.
    pub(super) fn new() -> Self {
        Self {
            facts: Vec::new(),
            paths: RefCell::new(PathInterner::new()),
            valid: true,
        }
    }

    pub(super) fn push(&mut self, fact: SemanticFact) {
        // Once an invariant is broken, discard subsequent input rather than
        // exposing a partially trustworthy stream to matcher indexes.
        if !self.valid || self.facts.len() >= MAX_FACTS {
            self.valid = false;
            return;
        }
        if fact.id() != FactId::from_index(self.facts.len()).unwrap_or(FactId(u32::MAX)) {
            self.valid = false;
            return;
        }
        self.facts.push(fact);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }

    pub(super) fn is_valid(&self) -> bool {
        self.valid
    }

    pub(in crate::analysis) fn fact(&self, id: FactId) -> Option<&SemanticFact> {
        self.facts.get(id.index()?)
    }

    pub(in crate::analysis) fn paths(&self) -> std::cell::Ref<'_, PathInterner> {
        self.paths.borrow()
    }

    pub(in crate::analysis) fn intern_path(
        &self,
        parent: PathId,
        segment: PathSegment,
    ) -> Option<PathId> {
        // Path interning is the one mutable sub-index needed after the fact
        // walk. RefCell keeps the public stream immutable to query callers.
        self.paths.borrow_mut().append(parent, segment)
    }

    pub(in crate::analysis) fn concat_paths(
        &self,
        prefix: PathId,
        suffix: PathId,
    ) -> Option<PathId> {
        self.paths.borrow_mut().concat(prefix, suffix)
    }

    #[cfg(test)]
    pub(super) fn facts_at(&self, lo: BytePos, hi: BytePos, kind: FactKind) -> Vec<&SemanticFact> {
        self.facts
            .iter()
            .filter(|fact| fact.span.lo() == lo && fact.span.hi() == hi && fact.kind() == kind)
            .collect()
    }

    pub(in crate::analysis) fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    #[cfg(test)]
    pub(super) fn fingerprint(&self) -> String {
        format!("{:?}", self.facts)
    }
}
