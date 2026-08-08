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

use glass_lint_datastructures::{NameTable, PathId, PathSegment, PathSegmentInput, PathStore};

use crate::analysis::{
    facts::{FactId, FactPayload, MAX_FACTS, ParameterBinding, SemanticFact},
    model::{
        fact::{Building, Frozen},
        scope::FunctionId,
        value::ValueTable,
    },
    resolution::FrozenFactTables,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FactStreamIssue {
    BudgetExhausted,
    PathExhausted,
    InvalidParserSpan,
    NameExhausted,
}

/// Construction authority held only by the building fact stream.
#[derive(Debug)]
pub(in crate::analysis) struct FactStreamToken(());

impl FactStreamToken {
    fn new() -> Self {
        Self(())
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn for_test() -> Self {
        Self(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FactStreamIssueSet(u8);

impl FactStreamIssueSet {
    const fn new() -> Self {
        Self(0)
    }

    fn insert(&mut self, issue: FactStreamIssue) {
        self.0 |= 1 << (issue as u8);
    }

    fn contains(self, issue: FactStreamIssue) -> bool {
        self.0 & (1 << (issue as u8)) != 0
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug)]
pub(in crate::analysis) struct BuildingStorage;

#[derive(Debug)]
pub(in crate::analysis) struct FrozenStorage {
    names: NameTable,
    values: ValueTable,
}

impl FrozenStorage {
    pub(in crate::analysis) fn from_tables(names: NameTable, values: ValueTable) -> Self {
        Self { names, values }
    }
}

pub(in crate::analysis) trait FactPhase {
    type Storage: std::fmt::Debug;
}

impl FactPhase for Building {
    type Storage = BuildingStorage;
}

impl FactPhase for Frozen {
    type Storage = FrozenStorage;
}

#[derive(Debug)]
/// Canonical facts plus the path interner used by argument and flow queries.
/// Invalid streams are retained only as a diagnostic boundary and must not be
/// indexed or projected as if their suffix were trustworthy.
///
/// The `Phase` parameter distinguishes the mutable building phase
/// ([`Building`]) from the frozen phase ([`Frozen`]). Resolver-owned names and
/// values exist only in the frozen phase, where
/// [`freeze`](FactStream<Building>::freeze) installs them.
pub(in crate::analysis) struct FactStream<Phase: FactPhase = Building> {
    /// Dense facts in canonical visitor order.
    facts: Vec<SemanticFact>,
    max_facts: usize,
    /// Interned property/index paths used by argument projections.
    paths: PathStore,
    /// Phase-owned storage: empty while building, resolver tables when frozen.
    storage: Phase::Storage,
    /// Canonical function parameter bindings indexed by FunctionId. Populated
    /// during building; effects and summaries look up bindings here instead of
    /// cloning from inline fact payloads.
    function_parameters: Vec<Vec<ParameterBinding>>,
    /// False after any ID, budget, or append invariant is violated.
    valid: bool,
    /// Typed construction outcomes that make the retained stream incomplete.
    issues: FactStreamIssueSet,
    /// Phase marker, zero-sized.
    _phase: PhantomData<Phase>,
}

// ── Shared methods (available in all phases) ────────────────────────────

impl<T: FactPhase> FactStream<T> {
    /// Whether every appended fact has satisfied the stream invariants.
    pub(in crate::analysis) fn is_valid(&self) -> bool {
        self.valid && self.issues.is_empty()
    }

    pub(in crate::analysis) fn is_structurally_valid(&self) -> bool {
        self.valid
    }

    pub(in crate::analysis) fn name_exhausted(&self) -> bool {
        self.issues.contains(FactStreamIssue::NameExhausted)
    }

    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.issues.contains(FactStreamIssue::BudgetExhausted)
    }

    pub(in crate::analysis) fn path_exhausted(&self) -> bool {
        self.issues.contains(FactStreamIssue::PathExhausted)
    }

    pub(in crate::analysis) fn invalid_parser_span(&self) -> bool {
        self.issues.contains(FactStreamIssue::InvalidParserSpan)
    }

    /// Look up a fact by its bounded dense identity.
    pub(in crate::analysis) fn fact(&self, id: FactId) -> Option<&SemanticFact> {
        self.facts.get(id.index()?)
    }

    /// The number of functions registered in the stream.
    ///
    /// At minimum 1 (the implicit program-level function at [`FunctionId`] 0)
    /// even when no user-defined functions exist.
    pub(in crate::analysis) fn function_count(&self) -> usize {
        self.function_parameters.len().max(1)
    }

    /// Borrow the canonical path table for read-only projection queries.
    pub(in crate::analysis) fn paths(&self) -> &PathStore {
        &self.paths
    }

    /// Look up the canonical parameter bindings for a function. Returns an
    /// empty slice when the function has no registered parameters (e.g. the
    /// program-level slot or an exit fact).
    pub(in crate::analysis) fn function_parameters(&self, id: FunctionId) -> &[ParameterBinding] {
        let Ok(index) = usize::try_from(id.raw()) else {
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
    ) -> Option<crate::analysis::model::value::ValueId> {
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
            paths: PathStore::new(),
            storage: BuildingStorage,
            function_parameters: Vec::new(),
            valid: true,
            issues: FactStreamIssueSet::new(),
            _phase: PhantomData,
        }
    }

    pub(super) fn append(
        &mut self,
        span: glass_lint_datastructures::ByteRange,
        function: FunctionId,
        payload: FactPayload,
    ) {
        // Once an invariant is broken, discard subsequent input rather than
        // exposing a partially trustworthy stream to matcher indexes.
        if !self.valid || self.facts.len() >= self.max_facts {
            self.valid = false;
            self.mark_budget_exhausted();
            return;
        }
        let Ok(raw_id) = u32::try_from(self.facts.len()) else {
            self.valid = false;
            self.mark_budget_exhausted();
            return;
        };
        let id = FactId::new(raw_id);
        let fact = SemanticFact::new(FactStreamToken::new(), id, span, function, payload);
        self.facts.push(fact);
    }

    #[cfg(test)]
    pub(super) fn push(&mut self, fact: SemanticFact) {
        if !self.valid || self.facts.len() >= self.max_facts {
            self.valid = false;
            return;
        }
        if fact.id().raw() as usize != self.facts.len() {
            self.valid = false;
            return;
        }
        self.facts.push(fact);
    }

    pub(in crate::analysis) fn max_facts(&self) -> usize {
        self.max_facts
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
        let index = usize::try_from(id.raw()).expect("FunctionId fits in usize");
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

    /// Consume the building stream and return a frozen stream with the
    /// resolver-owned name/value tables permanently attached.
    pub(in crate::analysis) fn freeze(self, tables: FrozenFactTables) -> FactStream<Frozen> {
        FactStream {
            facts: self.facts,
            max_facts: self.max_facts,
            paths: self.paths,
            storage: tables.into_storage(),
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
        &self.storage.names
    }

    /// Borrow the frozen value arena for shape lookups by ValueId.
    pub(in crate::analysis) fn values(&self) -> &ValueTable {
        &self.storage.values
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
impl Default for FactStream<Frozen> {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            max_facts: MAX_FACTS,
            paths: PathStore::default(),
            storage: FrozenStorage {
                names: NameTable::default(),
                values: ValueTable::default(),
            },
            function_parameters: Vec::new(),
            valid: true,
            issues: FactStreamIssueSet::new(),
            _phase: PhantomData,
        }
    }
}
