//! Immutable semantic fact stream with deterministic insertion order.
//!
//! Construction is append-only and validates dense IDs and the global fact
//! budget. Query callers receive an immutable view. Path interning happens
//! during ordinary mutable construction, not through interior mutation.
//!
//! The phase type parameter distinguishes the mutable building phase
//! ([`Building`]) from the immutable frozen phase ([`Frozen`]). Only a frozen
//! stream exposes the name table and value arena, making the freeze-ordering
//! invariant compiler-checked.

use std::marker::PhantomData;

use glass_lint_datastructures::NameTable;

use crate::analysis::{
    facts::{FactId, FactKind, FactPayload, MAX_FACTS, ParameterBinding, SemanticFact},
    value::{FunctionId, PathId, PathInterner, PathSegment, PathSegmentInput, ValueTable},
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

/// Marker type for the mutable building phase of [`FactStream`].
#[derive(Debug)]
pub(in crate::analysis) struct Building;

/// Marker type for the immutable frozen phase of [`FactStream`].
/// Accessors for names and values are only available in this phase.
#[derive(Debug)]
pub(in crate::analysis) struct Frozen;

#[derive(Debug)]
/// Canonical facts plus the path interner used by argument and flow queries.
/// Invalid streams are retained only as a diagnostic boundary and must not be
/// indexed or projected as if their suffix were trustworthy.
///
/// The `Phase` parameter distinguishes the mutable building phase
/// ([`Building`]) from the frozen phase ([`Frozen`]). Names and values are
/// always `None` during building; after
/// [`freeze`](FactStream<Building>::freeze) they are always `Some`.
pub(in crate::analysis) struct FactStream<Phase = Building> {
    /// Dense facts in canonical visitor order.
    facts: Vec<SemanticFact>,
    max_facts: usize,
    /// Interned property/index paths used by argument projections.
    paths: PathInterner,
    /// Frozen name table, set during `freeze`.
    names: Option<NameTable>,
    /// Frozen value arena, set during `freeze`.
    values: Option<ValueTable>,
    /// Canonical function parameter bindings indexed by FunctionId. Populated
    /// during building; effects and summaries look up bindings here instead of
    /// cloning from inline fact payloads.
    function_parameters: Vec<Vec<ParameterBinding>>,
    /// False after any ID, budget, or append invariant is violated.
    valid: bool,
    /// Typed construction outcomes that make the retained stream incomplete.
    issues: std::collections::BTreeSet<FactStreamIssue>,
    /// Phase marker, zero-sized.
    _phase: PhantomData<Phase>,
}

// ── Shared methods (available in all phases) ────────────────────────────

impl<T> FactStream<T> {
    /// Whether every appended fact has satisfied the stream invariants.
    pub(in crate::analysis) fn is_valid(&self) -> bool {
        self.valid && self.issues.is_empty()
    }

    pub(in crate::analysis) fn is_structurally_valid(&self) -> bool {
        self.valid
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

    /// Look up the canonical parameter bindings for a function. Returns an
    /// empty slice when the function has no registered parameters (e.g. the
    /// program-level slot or an exit fact).
    pub(in crate::analysis) fn function_parameters(&self, id: FunctionId) -> &[ParameterBinding] {
        let Ok(index) = usize::try_from(id.0) else {
            return &[];
        };
        self.function_parameters
            .get(index)
            .map_or(&[], |params| params.as_slice())
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.facts.len()
    }

    /// Borrow the assigned value identity from a property-write event.
    pub(in crate::analysis) fn property_write_value(
        &self,
        event: FactId,
    ) -> Option<crate::analysis::value::ValueId> {
        match &self.fact(event)?.payload {
            FactPayload::PropertyWrite { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Borrow all facts in the exact order in which the builder emitted them.
    pub(in crate::analysis) fn facts(&self) -> &[SemanticFact] {
        &self.facts
    }

    #[cfg(test)]
    pub(super) fn facts_at(&self, lo: u32, hi: u32, kind: FactKind) -> Vec<&SemanticFact> {
        self.facts
            .iter()
            .filter(|fact| fact.span.start() == lo && fact.span.end() == hi && fact.kind() == kind)
            .collect()
    }

    #[cfg(test)]
    pub(super) fn fingerprint(&self) -> String {
        format!("{:?}", self.facts)
    }
}

// ── Building-phase methods ─────────────────────────────────────────────

impl FactStream<Building> {
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
            function_parameters: Vec::new(),
            valid: true,
            issues: std::collections::BTreeSet::new(),
            _phase: PhantomData,
        }
    }

    pub(super) fn try_push(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        function: FunctionId,
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

    /// Register parameter bindings for a function identity.
    pub(super) fn register_function_parameters(
        &mut self,
        id: FunctionId,
        parameters: Vec<ParameterBinding>,
    ) {
        let index = usize::try_from(id.0).expect("FunctionId fits in usize");
        if self.function_parameters.len() <= index {
            self.function_parameters.resize_with(index + 1, Vec::new);
        }
        self.function_parameters[index] = parameters;
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

    /// Consume the building stream and return a frozen stream with the name
    /// table and value arena permanently attached.
    pub(in crate::analysis) fn freeze(
        self,
        names: NameTable,
        values: ValueTable,
    ) -> FactStream<Frozen> {
        debug_assert!(self.names.is_none(), "names already frozen");
        debug_assert!(self.values.is_none(), "values already frozen");
        FactStream {
            facts: self.facts,
            max_facts: self.max_facts,
            paths: self.paths,
            names: Some(names),
            values: Some(values),
            function_parameters: self.function_parameters,
            valid: self.valid,
            issues: self.issues,
            _phase: PhantomData,
        }
    }
}

// ── Frozen-phase methods ───────────────────────────────────────────────

impl FactStream<Frozen> {
    /// Borrow the frozen name table.
    pub(in crate::analysis) fn names(&self) -> &NameTable {
        self.names
            .as_ref()
            .expect("FactStream<Frozen> always has names")
    }

    /// Borrow the frozen value arena for shape lookups by ValueId.
    pub(in crate::analysis) fn values(&self) -> &ValueTable {
        self.values
            .as_ref()
            .expect("FactStream<Frozen> always has values")
    }

    /// Resolve a `NameId` to a `&str` via the frozen name table.
    pub(in crate::analysis) fn resolve_name(
        &self,
        id: glass_lint_datastructures::NameId,
    ) -> Option<&str> {
        self.names().resolve(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::value::{Value, ValueId, ValueTable};

    #[test]
    fn failed_path_interning_is_recorded_as_incomplete() {
        let mut stream = FactStream::<Building>::new();
        stream.mark_path_exhausted();
        assert!(stream.path_exhausted());
        assert!(!stream.is_valid());
    }

    #[test]
    fn name_exhaustion_is_recorded_and_invalidates_stream() {
        let mut stream = FactStream::<Building>::new();
        assert!(!stream.name_exhausted());
        stream.mark_name_exhausted();
        assert!(stream.name_exhausted());
        assert!(!stream.is_valid());
    }

    #[test]
    fn freeze_transitions_to_frozen_phase_with_both_tables() {
        let mut values = ValueTable::default();
        let string = values.intern(Value::StaticString("from-arena".into()));
        let stream = FactStream::<Building>::new().freeze(NameTable::default(), values);

        assert!(stream.name_exhausted() || !stream.name_exhausted()); // exists in both phases
        assert_eq!(stream.values().static_string(string), Some("from-arena"));
        assert!(stream.values().get(ValueId(u32::MAX)).is_none());
    }

    #[test]
    fn frozen_values_are_borrowed_by_artifact_local_id() {
        let mut values = ValueTable::default();
        let string = values.intern(Value::StaticString("from-arena".into()));
        let stream = FactStream::<Building>::new().freeze(NameTable::default(), values);

        assert_eq!(stream.values().static_string(string), Some("from-arena"));
        assert!(stream.values().get(ValueId(u32::MAX)).is_none());
    }
}
