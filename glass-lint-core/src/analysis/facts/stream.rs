//! Immutable semantic fact stream with deterministic insertion order.
//!
//! Construction is append-only and validates dense IDs and the global fact
//! budget. Query callers receive an immutable view. Path interning happens
//! during ordinary mutable construction, not through interior mutation.

#[cfg(test)]
use crate::analysis::facts::FactKind;
use crate::analysis::{
    facts::{FactId, MAX_FACTS, SemanticFact},
    name::NameTable,
    value::{PathId, PathInterner, PathSegment, PathSegmentInput},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FactStreamIssue {
    BudgetExhausted,
    PathExhausted,
    InvalidParserSpan,
    NameExhausted,
}

#[derive(Debug)]
/// Canonical facts plus the path interner used by argument and flow queries.
/// Invalid streams are retained only as a diagnostic boundary and must not be
/// indexed or projected as if their suffix were trustworthy.
pub(in crate::analysis) struct FactStream {
    /// Dense facts in canonical visitor order.
    facts: Vec<SemanticFact>,
    /// Interned property/index paths used by argument projections.
    paths: PathInterner,
    /// Frozen table owned directly by the stream.
    names: Option<NameTable>,
    /// False after any ID, budget, or append invariant is violated.
    valid: bool,
    /// Typed construction outcomes that make the retained stream incomplete.
    issues: std::collections::BTreeSet<FactStreamIssue>,
}

impl FactStream {
    /// Create an empty, valid stream. Fact IDs are assigned by the builder;
    /// this type verifies the resulting sequence as facts are appended.
    pub(super) fn new() -> Self {
        Self {
            facts: Vec::new(),
            paths: PathInterner::new(),
            names: None,
            valid: true,
            issues: std::collections::BTreeSet::new(),
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
    pub(in crate::analysis) fn is_valid(&self) -> bool {
        self.valid && self.issues.is_empty()
    }

    pub(in crate::analysis) fn is_structurally_valid(&self) -> bool {
        self.valid
    }

    pub(super) fn mark_budget_exhausted(&mut self) {
        self.issues.insert(FactStreamIssue::BudgetExhausted);
    }

    pub(super) fn mark_path_exhausted(&mut self) {
        self.issues.insert(FactStreamIssue::PathExhausted);
    }

    pub(super) fn mark_invalid_parser_span(&mut self) {
        self.issues.insert(FactStreamIssue::InvalidParserSpan);
    }

    pub(in crate::analysis) fn mark_name_exhausted(&mut self) {
        self.issues.insert(FactStreamIssue::NameExhausted);
    }

    pub(in crate::analysis) fn freeze_names(&mut self, names: NameTable) -> Result<(), NameTable> {
        if self.names.is_some() {
            return Err(names);
        }
        self.names = Some(names);
        Ok(())
    }

    pub(in crate::analysis) fn names(&self) -> Option<&NameTable> {
        self.names.as_ref()
    }

    pub(in crate::analysis) fn resolve_name(
        &self,
        id: crate::analysis::name::NameId,
    ) -> Option<&str> {
        self.names()?.resolve(id)
    }

    pub(in crate::analysis) fn name_exhausted(&self) -> bool {
        self.issues.contains(&FactStreamIssue::NameExhausted)
    }

    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.issues.contains(&FactStreamIssue::BudgetExhausted)
    }

    pub(in crate::analysis) fn path_exhausted(&self) -> bool {
        self.issues.contains(&FactStreamIssue::PathExhausted)
    }

    pub(in crate::analysis) fn invalid_parser_span(&self) -> bool {
        self.issues.contains(&FactStreamIssue::InvalidParserSpan)
    }

    /// Look up a fact by its bounded dense identity.
    pub(in crate::analysis) fn fact(&self, id: FactId) -> Option<&SemanticFact> {
        self.facts.get(id.index()?)
    }

    /// Borrow the canonical path table for read-only projection queries.
    pub(in crate::analysis) fn paths(&self) -> &PathInterner {
        &self.paths
    }

    pub(super) fn intern_path_input(
        &mut self,
        parent: PathId,
        segment: PathSegmentInput<'_>,
    ) -> Option<PathId> {
        let segment = match segment {
            PathSegmentInput::Property(_) => return None,
            PathSegmentInput::PropertyId(name) => PathSegment::Property(name),
            PathSegmentInput::Index(index) => PathSegment::Index(index),
        };
        self.paths.append(parent, segment)
    }

    #[cfg(test)]
    pub(super) fn facts_at(&self, lo: u32, hi: u32, kind: FactKind) -> Vec<&SemanticFact> {
        self.facts
            .iter()
            .filter(|fact| fact.span.start() == lo && fact.span.end() == hi && fact.kind() == kind)
            .collect()
    }

    /// Borrow the effective call arguments for a call event from the stream.
    pub(in crate::analysis) fn call_args_for_event(
        &self,
        event: crate::analysis::facts::FactId,
    ) -> Option<&[crate::analysis::facts::CallArgInfo]> {
        let fact = self.fact(event)?;
        match &fact.payload {
            crate::analysis::facts::FactPayload::Call { args, unwrap, .. } => Some(
                unwrap
                    .as_deref()
                    .map_or(args.as_slice(), |u| u.effective_args.as_slice()),
            ),
            _ => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_path_interning_is_recorded_as_incomplete() {
        let mut stream = FactStream::new();
        stream.mark_path_exhausted();
        assert!(stream.path_exhausted());
        assert!(!stream.is_valid());
    }

    #[test]
    fn name_exhaustion_is_rejected_before_indexing() {
        let mut stream = FactStream::new();
        assert!(stream.names().is_none());
        stream.mark_name_exhausted();
        assert!(stream.name_exhausted());
        assert!(!stream.is_valid());
    }

    #[test]
    fn names_can_only_be_frozen_once() {
        let mut stream = FactStream::new();
        assert!(stream.freeze_names(NameTable::default()).is_ok());
        assert!(stream.freeze_names(NameTable::default()).is_err());
        assert!(stream.names().is_some());
    }
}
