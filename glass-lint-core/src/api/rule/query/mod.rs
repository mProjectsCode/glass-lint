//! Declaration-owned query semantics and typed logical query algebra.
//!
//! Types in this module are provider-neutral and validated at construction.
//! They represent authored intent without exposing compiler IR. The compiler
//! layer lowers these into physical execution plans.
//!
//! The primary authoring API consists of constructor methods on [`EventQuery`].
//! Rule authors create event queries and pass them directly to
//! [`crate::api::rule::RuleBuilder::query`] (via the [`IntoQueryDecl`] adapter)
//! or convert them with [`EventQuery::into_query`] when composing alternatives
//! or conjunctions.
//!
//! [`EventQuery::call_global`] and the other identity/event combinators replace
//! the former [`QueryDecl`] builder.
use std::fmt;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ModuleSpecifierPattern,
        query::{
            lifecycle::{LifecycleCompletion, LifecycleCondition},
            value::{
                ArgumentConstraint, ArgumentConstraintsBuilder, ArgumentIndex, ArgumentMatcher,
                ValueMatcher,
            },
        },
    },
};

mod composition;
mod constructors;
pub(crate) mod expression;
pub(crate) mod lifecycle;
pub(crate) mod limits;
pub(crate) mod value;
pub use expression::{AllExpr, AnyExpr, QueryExpr};
pub(crate) use expression::{QueryExprKind, QueryShapeFacts};

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
pub struct VarId(u32);

impl VarId {
    /// Create a variable ID from a raw index.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Return the raw index.
    pub fn get(self) -> u32 {
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventQuery {
    /// Variable bound by this event selection.
    var: VarId,
    /// Kind of event (call, construct, member call, etc.).
    event: EventSpec,
    /// Identity specification (global, rooted, module, etc.).
    identity: IdentitySpec,
    /// Argument value constraints (empty for non-call events).
    constraints: Vec<ArgumentConstraint>,
}

impl EventQuery {
    pub fn var(&self) -> VarId {
        self.var
    }

    pub(crate) fn event(&self) -> &EventSpec {
        &self.event
    }

    pub(crate) fn identity(&self) -> &IdentitySpec {
        &self.identity
    }

    pub fn constraints(&self) -> &[ArgumentConstraint] {
        &self.constraints
    }

    /// Construct the invariant-empty event-query shell used by every public
    /// event constructor. Argument constraints are added only through the
    /// validated adapter methods below.
    fn from_parts(event: EventSpec, identity: IdentitySpec) -> Self {
        Self {
            var: VarId::new(0),
            event,
            identity,
            constraints: Vec::new(),
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
            constraints,
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

fn is_chain_malformed(chain: &str) -> bool {
    chain.trim().is_empty()
        || chain.contains("..")
        || chain.starts_with('.')
        || chain.ends_with('.')
}

/// A member chain validated once at the query boundary, retaining both its
/// canonical display spelling and the parsed symbol path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct MemberChain {
    display: String,
    path: SymbolPath,
}

impl MemberChain {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, QueryBuildError> {
        let value = value.into();
        if is_chain_malformed(&value) {
            return Err(QueryBuildError::MalformedChain(value));
        }
        let path = SymbolPath::from_chain(&value);
        if path.is_empty() {
            return Err(QueryBuildError::MalformedChain(value));
        }
        Ok(Self {
            display: path.to_string(),
            path,
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.display
    }

    pub(crate) fn path(&self) -> &SymbolPath {
        &self.path
    }

    pub(crate) fn into_path(self) -> SymbolPath {
        self.path
    }
}

fn checked_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    let value: SmolStr = value.into().trim().to_owned().into();
    if value.trim().is_empty() {
        return Err(QueryBuildError::EmptyIdentityName);
    }
    Ok(value)
}

pub(super) fn checked_module_name(value: impl Into<String>) -> Result<SmolStr, QueryBuildError> {
    let value: SmolStr = value.into().trim().to_owned().into();
    if value.is_empty() {
        return Err(QueryBuildError::EmptyModuleSpecifier);
    }
    Ok(value)
}

fn checked_module_export(
    module: impl Into<String>,
    export: impl Into<String>,
) -> Result<(SmolStr, SmolStr), QueryBuildError> {
    let module = checked_module_name(module)?;
    let export = checked_name(export)?;
    Ok((module, export))
}

pub(crate) fn checked_chain(value: impl Into<String>) -> Result<MemberChain, QueryBuildError> {
    MemberChain::parse(value)
}

pub(crate) const PRIVATE_NETWORK_LITERAL: &str = "__glass_lint_private_network_literal__";
pub(crate) const PRIVATE_NETWORK_EVIDENCE_SYMBOL: &str = "private network address";

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
    /// Optional completion mode (sink or configuration).
    completion: Option<LifecycleCompletion>,
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

    pub fn completion(&self) -> Option<&LifecycleCompletion> {
        self.completion.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn from_parts_for_test(
        symbol: impl Into<String>,
        sources: Vec<EventQuery>,
        condition: Option<LifecycleCondition>,
        completion: Option<LifecycleCompletion>,
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
    pub fn primary_var(&self) -> VarId {
        self.primary_var
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
        if index > limits::MAX_ARGUMENT_INDEX {
            return Err(QueryBuildError::InvalidArgumentIndex(index));
        }
        #[allow(clippy::cast_possible_truncation)]
        let idx = index as u8;
        Ok(Self {
            kind: EventRequirementKind::Argument {
                index: ArgumentIndex::new_unchecked(idx),
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

fn explain_expression(expression: &QueryExpr) -> String {
    match expression.kind() {
        QueryExprKind::Event(query) => explain_event(query),
        QueryExprKind::SelectEvent(selection) => format!("event {} is selected", selection.bind),
        QueryExprKind::Require(predicate) => explain_predicate(predicate),
        QueryExprKind::Any(any) => format!(
            "any of: {}",
            any.iter()
                .map(explain_expression)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        QueryExprKind::All(all) => format!(
            "all of: {}",
            all.iter()
                .map(explain_expression)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        QueryExprKind::Lifecycle(lifecycle) => explain_lifecycle(lifecycle),
    }
}

fn explain_event(query: &EventQuery) -> String {
    let event = match &query.event {
        EventSpec::Call => format!("a call to {}", explain_identity(&query.identity)),
        EventSpec::Construct => format!(
            "a constructor call to {}",
            explain_identity(&query.identity)
        ),
        EventSpec::MemberCall { member } => format!(
            "a member call to `{member}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::MemberRead { member } => format!(
            "a member read of `{member}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::PropertyWrite { property } => format!(
            "a property write to `{property}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::ClassReference => {
            format!("a class reference to {}", explain_identity(&query.identity))
        }
        EventSpec::Import => format!("an import of {}", explain_identity(&query.identity)),
        EventSpec::StringReference => {
            format!(
                "a string reference containing `{}`",
                query.identity.display_name()
            )
        }
    };
    append_constraints(event, &query.constraints)
}

fn explain_identity(identity: &IdentitySpec) -> String {
    match identity {
        IdentitySpec::Global { name } => format!("the global `{name}`"),
        IdentitySpec::Heuristic { name } => format!("the heuristic name `{name}`"),
        IdentitySpec::ModuleExport { module, export } => {
            format!("the `{export}` export from module `{module}`")
        }
        IdentitySpec::PackageModuleExport { module, export } => {
            format!("the `{export}` export from package/module `{module}`")
        }
        IdentitySpec::ModuleNamespace { module } => {
            format!("the namespace imported from module `{module}`")
        }
        IdentitySpec::PackageModuleNamespace { module } => {
            format!("the namespace imported from package/module `{module}`")
        }
        IdentitySpec::Rooted { path } => format!("the rooted path `{path}`"),
        IdentitySpec::LiteralString { predicate } => format!("a string matching `{predicate}`"),
        IdentitySpec::PackageSpecifier { pattern } => {
            format!("the package specifier `{pattern}`")
        }
    }
}

fn append_constraints(mut description: String, constraints: &[ArgumentConstraint]) -> String {
    if !constraints.is_empty() {
        let rendered = constraints
            .iter()
            .map(|constraint| {
                format!(
                    "argument {} matches {}",
                    constraint.index(),
                    explain_argument_matcher(constraint.predicate())
                )
            })
            .collect::<Vec<_>>()
            .join(" and ");
        description.push_str(" with ");
        description.push_str(&rendered);
    }
    description
}

fn explain_predicate(predicate: &QueryPredicate) -> String {
    match predicate {
        QueryPredicate::EventKind { event, expected } => match expected {
            EventSpec::MemberCall { member } => {
                format!("event {event} is a member call to `{member}`")
            }
            EventSpec::MemberRead { member } => {
                format!("event {event} is a member read of `{member}`")
            }
            expected => format!("event {event} is a {}", expected.diagnostic_name()),
        },
        QueryPredicate::EventIdentity { event, expected } => {
            format!("event {event} has identity {}", explain_identity(expected))
        }
        QueryPredicate::Argument {
            call,
            index,
            matcher,
        } => format!(
            "argument {}[{}] matches {}",
            call,
            index.get(),
            explain_argument_matcher(matcher)
        ),
        QueryPredicate::ReturnedObject { bind, identity } => {
            format!(
                "object {bind} is returned by {}",
                explain_identity(identity)
            )
        }
        QueryPredicate::ConstructedObject { bind, identity } => {
            format!(
                "object {bind} is constructed by {}",
                explain_identity(identity)
            )
        }
        QueryPredicate::MemberSubject { event, object } => {
            format!("event {event} uses object {object} as its member receiver")
        }
    }
}

fn explain_argument_matcher(matcher: &ArgumentMatcher) -> String {
    match matcher.kind() {
        value::ArgumentMatcherKind::Value(value) => explain_value_matcher(value),
        value::ArgumentMatcherKind::ObjectKeys(keys) => {
            format!("an object with keys {}", quoted_list(keys))
        }
        value::ArgumentMatcherKind::RootedExpressions(paths) => {
            format!("one of the rooted expressions {}", quoted_list(paths))
        }
        value::ArgumentMatcherKind::ObjectPropertyValue { property, value } => format!(
            "an object whose `{property}` property matches {}",
            explain_value_matcher(value)
        ),
    }
}

fn explain_value_matcher(matcher: &ValueMatcher) -> String {
    match matcher.kind() {
        value::ValueMatcherKind::Any => "any value".into(),
        value::ValueMatcherKind::StaticString(predicate) => match predicate.kind() {
            value::StaticStringPredicateKind::Any => "any static string".into(),
            value::StaticStringPredicateKind::Exact(values) => {
                format!("one of the exact strings {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::Prefix(values) => {
                format!("a string starting with one of {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::ContainsAny(values) => {
                format!("a string containing any of {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::ContainsAll(values) => {
                format!("a string containing all of {}", quoted_list(values))
            }
        },
    }
}

fn quoted_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn explain_lifecycle(lifecycle: &LifecycleQuery) -> String {
    let sources = lifecycle
        .sources()
        .iter()
        .map(explain_event)
        .collect::<Vec<_>>()
        .join("; ");
    let condition = lifecycle.condition().map_or_else(
        || "no configuration condition".into(),
        explain_lifecycle_condition,
    );
    let completion = lifecycle.completion().map_or_else(
        || "no completion condition".into(),
        explain_lifecycle_completion,
    );
    format!(
        "a lifecycle object produced by {sources}; it requires {condition}; it completes when {completion}"
    )
}

fn explain_lifecycle_condition(condition: &lifecycle::LifecycleCondition) -> String {
    let (join, events) = match condition.kind() {
        lifecycle::LifecycleConditionKind::AnyOf(events) => ("any of", events),
        lifecycle::LifecycleConditionKind::AllOf(events) => ("all of", events),
    };
    format!(
        "{join} {}",
        events
            .iter()
            .map(explain_lifecycle_event)
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn explain_lifecycle_event(event: &lifecycle::LifecycleEvent) -> String {
    match event.kind() {
        lifecycle::LifecycleEventKind::PropertyWrite { property, value } => format!(
            "a write to `{property}` matching {}",
            explain_value_matcher(value)
        ),
        lifecycle::LifecycleEventKind::MemberCall { member, arguments } => {
            append_constraints(format!("a member call to `{}`", member.as_str()), arguments)
        }
    }
}

fn explain_lifecycle_completion(completion: &lifecycle::LifecycleCompletion) -> String {
    match completion.kind() {
        lifecycle::LifecycleCompletionKind::Configuration => "the configuration condition".into(),
        lifecycle::LifecycleCompletionKind::AnySink(sinks) => format!(
            "any sink {} receives the object",
            sinks
                .iter()
                .map(explain_lifecycle_sink)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        lifecycle::LifecycleCompletionKind::AllSinks(sinks) => format!(
            "all sinks {} receive the object",
            sinks
                .iter()
                .map(explain_lifecycle_sink)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn explain_lifecycle_sink(sink: &lifecycle::LifecycleSink) -> String {
    match sink.kind() {
        lifecycle::LifecycleSinkKind::ArgumentOf { endpoint, index } => {
            format!("`{}` argument {index}", endpoint.chain())
        }
        lifecycle::LifecycleSinkKind::AnyArgumentOf { endpoint } => {
            format!("any argument of `{}`", endpoint.chain())
        }
    }
}

#[cfg(test)]
mod tests;
