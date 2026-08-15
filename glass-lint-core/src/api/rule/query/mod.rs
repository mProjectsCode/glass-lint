//! Declaration-owned query semantics and typed logical query algebra.
//!
//! Types in this module are provider-neutral and validated at construction.
//! They represent authored intent without exposing compiler IR. The compiler
//! layer lowers these into physical execution plans.
//!
//! The primary authoring API consists of constructor methods on [`EventQuery`].
//! Declarative catalogs pass event queries directly to
//! [`crate::api::rule::CatalogRuleBuilder::query`] via the [`IntoQueryDecl`]
//! adapter; non-catalog code that propagates errors immediately uses
//! [`crate::api::rule::RuleBuilder::try_query`].
//! [`crate::api::rule::RuleBuilder::query`] accepts only a finished
//! [`QueryDecl`], and [`EventQuery::into_query`] converts an event query when
//! composing alternatives or conjunctions.
//!
//! [`EventQuery::call_global`] and the other identity/event combinators replace
//! the former [`QueryDecl`] builder.
use std::fmt;

use crate::api::{
    classification::MatchKind,
    rule::{
        ModuleSpecifierPattern,
        query::{
            lifecycle::{LifecycleCompletion, LifecycleCondition},
            value::{
                ArgumentConstraint, ArgumentConstraints, ArgumentIndex, ArgumentMatcher,
                ValueMatcher,
            },
        },
    },
};

mod canonical;
mod composition;
mod constructors;
mod declarations;
mod explanation;
pub(crate) mod expression;
pub(crate) mod lifecycle;
pub(crate) mod limits;
pub(crate) mod value;
pub(crate) use declarations::{
    MemberChain, PRIVATE_NETWORK_EVIDENCE_SYMBOL, checked_chain, checked_module_export,
    checked_module_name, checked_name,
};
pub(crate) use explanation::explain_expression;
pub use expression::QueryExpr;
pub(crate) use expression::{AllExpr, AnyExpr, QueryExprKind, QueryShapeFacts};

pub(crate) mod event;
pub(crate) use event::{EventSpec, IdentitySpec};
pub(crate) mod error;
pub use error::{QueryBuildError, QueryDiagnostic};

// ── Typed logical query algebra ───────────────────────────────────────

/// Stable semantic variable ID assigned in an authored query.
///
/// Variables are the main extensibility mechanism. Each variable has a
/// semantic type (event, object, identity, value) enforced by the predicates
/// that bind or constrain it. This ID belongs to the rule declaration and is
/// distinct from the private dense slots used by physical plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct VarId(u32);

impl VarId {
    /// Create a variable ID from a raw index.
    pub(crate) const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Return the raw index.
    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

/// Compiler-inferred variable type for type-checking passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum VarType {
    Event,
    CallEvent,
    MemberEvent,
    Object,
}

impl VarType {
    /// Return the stable diagnostic label for this inferred variable type.
    pub(crate) fn variant_name(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::CallEvent => "call_event",
            Self::MemberEvent => "member_event",
            Self::Object => "object",
        }
    }
}

/// Binding atom: selects one event variable at a call/member/import site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct EventSelection {
    pub(crate) bind: VarId,
}

/// A predicate that constrains an already-bound variable.
///
/// `ReturnedObject` and `ConstructedObject` each bind their declared object
/// variable. All other variants reference existing bindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum QueryPredicate {
    EventKind {
        event: VarId,
        expected: EventSpec,
    },
    EventIdentity {
        event: VarId,
        expected: IdentitySpec,
    },
    Argument {
        call: VarId,
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
    ReturnedObject {
        bind: VarId,
        identity: IdentitySpec,
    },
    ConstructedObject {
        bind: VarId,
        identity: IdentitySpec,
    },
    MemberSubject {
        event: VarId,
        object: VarId,
    },
}

/// A single event selection with identity and argument constraints.
///
/// This is a leaf predicate — it selects an event occurrence and optionally
/// constrains its identity and arguments. Evidence metadata is not stored
/// here; it lives in the query's emission metadata.
///
/// Construct instances through the typed combinator methods such as
/// [`EventQuery::call_global`], [`EventQuery::member_call_rooted`], etc.
/// Then call [`into_query`](Self::into_query) to produce a [`QueryDecl`]
/// with inferred evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventQuery {
    /// Variable bound by this event selection.
    var: VarId,
    /// Kind of event (call, construct, member call, etc.).
    event: EventSpec,
    /// Identity specification (global, rooted, module, etc.).
    identity: IdentitySpec,
    /// Argument value constraints (empty for non-call events).
    constraints: ArgumentConstraints,
}

impl EventQuery {
    pub(crate) fn var(&self) -> VarId {
        self.var
    }

    pub(crate) fn event(&self) -> &EventSpec {
        &self.event
    }

    pub(crate) fn identity(&self) -> &IdentitySpec {
        &self.identity
    }

    pub(crate) fn constraints(&self) -> &[ArgumentConstraint] {
        self.constraints.as_slice()
    }

    /// Return the number of argument constraints, kept sorted by argument
    /// index.
    #[must_use]
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// Construct the invariant-empty event-query shell used by every public
    /// event constructor. Argument constraints are added only through the
    /// validated adapter methods below.
    fn from_parts(event: EventSpec, identity: IdentitySpec) -> Self {
        Self {
            var: VarId::new(0),
            event,
            identity,
            constraints: ArgumentConstraints::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_parts_for_test(
        var: VarId,
        event: EventSpec,
        identity: IdentitySpec,
        constraints: Vec<ArgumentConstraint>,
    ) -> Self {
        Self {
            var,
            event,
            identity,
            constraints: ArgumentConstraints::from_constraints(constraints),
        }
    }
}

struct EventSelectionAssembly {
    event: EventQuery,
    emission: EmissionDecl,
}

impl EventQuery {
    fn into_selection_assembly(self) -> EventSelectionAssembly {
        let var = self.var;
        let kind = evidence_kind_for_event(&self.event);
        let symbol = self.identity.display_name();
        EventSelectionAssembly {
            event: self,
            emission: EmissionDecl {
                primary_var: var,
                kind,
                symbol,
            },
        }
    }
}

impl EventSelectionAssembly {
    fn var(&self) -> VarId {
        self.emission.primary_var
    }

    fn emission(&self) -> &EmissionDecl {
        &self.emission
    }

    fn into_event_decl(self) -> QueryDecl {
        QueryDecl {
            expression: QueryExpr::event(self.event),
            emission: self.emission,
        }
    }

    fn branches(&self) -> Vec<QueryExpr> {
        let var = self.event.var;
        let mut branches = vec![
            QueryExpr::select_event(var),
            QueryExpr::require(QueryPredicate::EventKind {
                event: var,
                expected: self.event.event.clone(),
            }),
            QueryExpr::require(QueryPredicate::EventIdentity {
                event: var,
                expected: self.event.identity.clone(),
            }),
        ];
        branches.extend(self.event.constraints.iter().map(|constraint| {
            QueryExpr::require(QueryPredicate::Argument {
                call: var,
                index: constraint.arg_index(),
                matcher: constraint.predicate().clone(),
            })
        }));
        branches
    }
}

fn evidence_kind_for_event(event: &EventSpec) -> MatchKind {
    match event {
        EventSpec::Call => MatchKind::Call,
        EventSpec::Construct => MatchKind::Constructor,
        EventSpec::MemberCall { .. } => MatchKind::MemberCall,
        EventSpec::MemberRead { .. } => MatchKind::MemberRead,
        EventSpec::PropertyWrite { .. } => MatchKind::PropertyWrite,
        EventSpec::ClassReference => MatchKind::Class,
        EventSpec::Import => MatchKind::Import,
        EventSpec::StringReference => MatchKind::StringContains,
    }
}

/// Object lifecycle: source events, condition, and completion.
///
/// Represents a bounded state machine tracking an object from its sources
/// (production), through configuration (requirements), to completion (sink or
/// self-configuration). The compiler validates stages and compiles to the
/// existing local/cross-call flow engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleQuery {
    /// Evidence symbol.
    symbol: String,
    /// Events that produce the tracked object.
    sources: Vec<EventQuery>,
    /// Optional configuration condition (requirements).
    condition: Option<LifecycleCondition>,
    /// Completion mode (sink or configuration).
    completion: LifecycleCompletion,
}

impl LifecycleQuery {
    /// Return the evidence symbol.
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn sources(&self) -> &[EventQuery] {
        &self.sources
    }

    pub fn condition(&self) -> Option<&LifecycleCondition> {
        self.condition.as_ref()
    }

    pub fn completion(&self) -> &LifecycleCompletion {
        &self.completion
    }

    #[cfg(test)]
    pub(crate) fn from_parts_for_test(
        symbol: impl Into<String>,
        sources: Vec<EventQuery>,
        condition: Option<LifecycleCondition>,
        completion: LifecycleCompletion,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            sources,
            condition,
            completion,
        }
    }
}

/// Evidence emission for a query result.
///
/// Every logical query must have exactly one emission declaration. It selects
/// the primary event variable and specifies the evidence kind and stable
/// symbol. The compiler validates that the primary variable exists on every
/// successful logical branch and has a source location.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmissionDecl {
    /// Variable holding the primary event for evidence.
    pub(crate) primary_var: VarId,
    /// Evidence kind (call, member call, flow, etc.).
    pub(crate) kind: MatchKind,
    /// Stable evidence symbol (e.g. "fetch", "document.createElement").
    pub(crate) symbol: String,
}

impl EmissionDecl {
    pub(crate) fn primary_var(&self) -> VarId {
        self.primary_var
    }

    pub(crate) fn is_compatible(&self, other: &Self) -> bool {
        self.primary_var == other.primary_var
            && self.kind == other.kind
            && self.symbol == other.symbol
    }

    pub(crate) fn is_compatible_with_aggregate_symbol(&self, other: &Self) -> bool {
        self.primary_var == other.primary_var && self.kind == other.kind
    }

    pub fn kind(&self) -> MatchKind {
        self.kind
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }
}

/// A single constraint on a previously selected event, used with
/// [`QueryDecl::all`] to compose same-event conjunctions.
///
/// Each `EventRequirement` is a typed predicate that references an
/// already-bound event variable. The compiler groups requirements by
/// argument index and canonicalizes them during normalization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventRequirement {
    pub(crate) kind: EventRequirementKind,
}

/// Interior kind for [`EventRequirement`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum EventRequirementKind {
    Argument {
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
}

impl EventRequirement {
    /// Create an argument constraint on the event.
    ///
    /// The `index` is the 0-based argument position. Returns
    /// [`QueryBuildError::InvalidArgumentIndex`] when the index exceeds
    /// the engine's bounded maximum argument index.
    pub fn argument(
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError> {
        let idx = ArgumentIndex::try_from_usize(index)?;
        Ok(Self {
            kind: EventRequirementKind::Argument {
                index: idx,
                matcher: matcher.into(),
            },
        })
    }
}

/// A full logical query: an expression with its emission declaration.
///
/// One `QueryDecl` corresponds conceptually to one authored matcher:
///
/// ```text
/// select ... where ... emit ...
/// ```
///
/// The expression selects and constrains events; the emission declaration
/// specifies how the result produces evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryDecl {
    /// The query expression (event, any, all, lifecycle).
    expression: QueryExpr,
    /// How to emit evidence from the result.
    emission: EmissionDecl,
}

#[cfg(test)]
impl QueryDecl {
    pub(crate) fn from_parts_for_test(expression: QueryExpr, emission: EmissionDecl) -> Self {
        Self {
            expression,
            emission,
        }
    }
}

/// Sealed trait allowing the rule builder's `query` method to accept a
/// `QueryDecl`, [`EventQuery`], or `Result` of either without requiring the
/// caller to unwrap.
pub trait IntoQueryDecl: private::Sealed {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError>;
}

impl IntoQueryDecl for QueryDecl {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        Ok(self)
    }
}

impl IntoQueryDecl for Result<QueryDecl, QueryBuildError> {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        self
    }
}

impl IntoQueryDecl for EventQuery {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        Ok(self.into_query())
    }
}

impl IntoQueryDecl for Result<EventQuery, QueryBuildError> {
    fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError> {
        self.map(EventQuery::into_query)
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for super::QueryDecl {}
    impl Sealed for Result<super::QueryDecl, super::QueryBuildError> {}
    impl Sealed for super::EventQuery {}
    impl Sealed for Result<super::EventQuery, super::QueryBuildError> {}
}

impl fmt::Display for QueryDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query emit={} {}", self.emission.symbol, self.expression)
    }
}

#[cfg(test)]
mod tests;
