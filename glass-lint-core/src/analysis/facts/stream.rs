//! Immutable semantic fact stream with deterministic insertion order.
//!
//! Construction is append-only and validates dense IDs and the global fact
//! budget. Query callers receive an immutable view; path interning is the only
//! interior mutation and is deterministic for the same traversal.

use std::cell::RefCell;

#[cfg(test)]
use swc_common::BytePos;

#[cfg(test)]
use super::FactKind;
use super::{FactId, MAX_FACTS, SemanticFact};
use crate::analysis::value::{PathId, PathInterner, PathSegment};

#[derive(Debug)]
/// Canonical facts plus the path interner used by argument and flow queries.
/// Invalid streams are retained only as a diagnostic boundary and must not be
/// indexed or projected as if their suffix were trustworthy.
pub(in crate::analysis) struct FactStream {
    /// Dense facts in canonical visitor order.
    facts: Vec<SemanticFact>,
    /// Interned property/index paths used by argument projections.
    paths: RefCell<PathInterner>,
    /// False after any ID, budget, or append invariant is violated.
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
            // A gap or duplicate ID would make indexed lookup disagree with
            // traversal order, so the whole stream becomes untrusted.
            self.valid = false;
            return;
        }
        self.facts.push(fact);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether every appended fact has satisfied the stream invariants.
    pub(super) fn is_valid(&self) -> bool {
        self.valid
    }

    /// Look up a fact by its bounded dense identity.
    pub(in crate::analysis) fn fact(&self, id: FactId) -> Option<&SemanticFact> {
        self.facts.get(id.index()?)
    }

    /// Borrow the canonical path table for read-only projection queries.
    pub(in crate::analysis) fn paths(&self) -> std::cell::Ref<'_, PathInterner> {
        self.paths.borrow()
    }

    /// Intern one path extension without exposing mutable stream state.
    pub(in crate::analysis) fn intern_path(
        &self,
        parent: PathId,
        segment: PathSegment,
    ) -> Option<PathId> {
        // Path interning is the one mutable sub-index needed after the fact
        // walk. RefCell keeps the public stream immutable to query callers.
        self.paths.borrow_mut().append(parent, segment)
    }

    /// Intern the concatenation of two previously validated paths.
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

    /// Borrow all facts in the exact order in which the builder emitted them.
    pub(in crate::analysis) fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    #[cfg(test)]
    pub(super) fn fingerprint(&self) -> String {
        format!("{:?}", self.facts)
    }
}
