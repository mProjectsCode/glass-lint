//! Immutable semantic fact stream with deterministic insertion order.
//!
//! Construction is append-only and validates dense IDs and the global fact
//! budget. Query callers receive an immutable view. Path interning happens
//! during ordinary mutable construction, not through interior mutation.

use crate::analysis::{
    facts::{FactId, FactKind, FactPayload, MAX_FACTS, SemanticFact},
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum FactIssue {
    BudgetExhausted,
}

#[derive(Debug)]
/// Canonical facts plus the path interner used by argument and flow queries.
/// Invalid streams are retained only as a diagnostic boundary and must not be
/// indexed or projected as if their suffix were trustworthy.
pub(in crate::analysis) struct FactStream {
    /// Dense facts in canonical visitor order.
    facts: Vec<SemanticFact>,
    max_facts: usize,
    /// Interned property/index paths used by argument projections.
    paths: PathInterner,
    /// Frozen name table owned directly by the stream after lowering.
    names: Option<NameTable>,
    /// Frozen value arena that remains authoritative for ValueId shapes after
    /// the resolver is dropped. Consumers borrow immutable value shapes through
    /// the stream instead of copying projections.
    values: Option<crate::analysis::value::ValueTable>,
    /// False after any ID, budget, or append invariant is violated.
    valid: bool,
    /// Typed construction outcomes that make the retained stream incomplete.
    issues: std::collections::BTreeSet<FactStreamIssue>,
}

impl FactStream {
    /// Create an empty, valid stream. Fact IDs are assigned by the builder;
    /// this type verifies the resulting sequence as facts are appended.
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::with_limit(MAX_FACTS)
    }

    pub(super) fn with_limit(max_facts: usize) -> Self {
        Self {
            facts: Vec::new(),
            max_facts: max_facts.min(MAX_FACTS),
            paths: PathInterner::new(),
            names: None,
            values: None,
            valid: true,
            issues: std::collections::BTreeSet::new(),
        }
    }

    pub(super) fn try_push(
        &mut self,
        span: crate::ByteRange,
        function: crate::analysis::value::FunctionId,
        kind: FactKind,
        payload: FactPayload,
    ) -> Result<FactId, FactIssue> {
        // Once an invariant is broken, discard subsequent input rather than
        // exposing a partially trustworthy stream to matcher indexes.
        if !self.valid || self.facts.len() >= self.max_facts {
            self.valid = false;
            self.mark_budget_exhausted();
            return Err(FactIssue::BudgetExhausted);
        }
        let id = FactId(u32::try_from(self.facts.len()).map_err(|_| {
            self.valid = false;
            self.mark_budget_exhausted();
            FactIssue::BudgetExhausted
        })?);
        let fact = SemanticFact::new(id, span, function, kind, payload);
        self.facts.push(fact);
        Ok(id)
    }

    #[cfg(test)]
    pub(super) fn push(&mut self, fact: SemanticFact) {
        if !self.valid || self.facts.len() >= self.max_facts {
            self.valid = false;
            return;
        }
        if fact.id().0 as usize != self.facts.len() {
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

    /// Freeze the interning arena so artifact consumers can borrow value shapes
    /// without re-evaluating or copying projections.
    pub(in crate::analysis) fn freeze_values(
        &mut self,
        values: crate::analysis::value::ValueTable,
    ) -> Result<(), crate::analysis::value::ValueTable> {
        if self.values.is_some() {
            return Err(values);
        }
        self.values = Some(values);
        Ok(())
    }

    /// Borrow the frozen value arena for shape lookups by ValueId.
    pub(in crate::analysis) fn values(&self) -> Option<&crate::analysis::value::ValueTable> {
        self.values.as_ref()
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

    /// Borrow the assigned value identity from a property-write event.
    pub(in crate::analysis) fn property_write_value(
        &self,
        event: crate::analysis::facts::FactId,
    ) -> Option<crate::analysis::value::ValueId> {
        match &self.fact(event)?.payload {
            crate::analysis::facts::FactPayload::PropertyWrite { value, .. } => Some(*value),
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

    #[test]
    fn frozen_values_are_borrowed_by_artifact_local_id() {
        use crate::analysis::value::{Value, ValueId, ValueTable};

        let mut values = ValueTable::default();
        let string = values.intern(Value::StaticString("from-arena".into()));
        let mut stream = FactStream::new();
        assert!(stream.freeze_values(values).is_ok());

        let values = stream.values().expect("values should be frozen");
        assert_eq!(values.static_string(string), Some("from-arena"));
        assert!(values.get(ValueId(u32::MAX)).is_none());
    }
}
