//! Declaration-owned query semantics and typed logical query algebra.
//!
//! Types in this module are provider-neutral and validated at construction by
//! the builder or lowered from [`MatcherDecl`]. They represent authored intent
//! without exposing compiler IR. The compiler layer lowers these into physical
//! execution plans.
//!
//! Phase 3 of the query architecture introduces:
//!
//! - [`VarId`] — dense compiler variable IDs for typed bindings;
//! - [`QueryExpr`] — a typed logical algebra with `Event`, `Any`, `All`, and
//!   `Lifecycle` operators;
//! - [`EmissionDecl`] — explicit evidence projection per query root;
//! - [`QueryDecl`] — one expression + emission pair;
//! - [`QuerySet`] — a rule's collection of independent queries; and
//! - [`QueryBuildError`] — construction-time errors for invalid query shapes.
//!
//! # Stability
//!
//! These types are defined in Phase 3 but are not yet consumed by the
//! compilation pipeline. Warnings about dead code are expected until Phase 4
//! (validation) and Phase 5 (normalization) integrate them.

#![allow(dead_code)]

use std::fmt;

use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    rule::{
        ArgumentConstraint, FlowCompletion, FlowCondition, MatcherDecl, ModuleSpecifierPattern,
    },
};

/// Declaration-owned identity specification for an event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum IdentitySpec {
    Global {
        name: SmolStr,
    },
    Heuristic {
        name: SmolStr,
    },
    ModuleExport {
        module: SmolStr,
        export: SmolStr,
    },
    PackageModuleExport {
        module: ModuleSpecifierPattern,
        export: SmolStr,
    },
    ModuleNamespace {
        module: SmolStr,
    },
    PackageModuleNamespace {
        module: ModuleSpecifierPattern,
    },
    Rooted {
        path: SymbolPath,
    },
    LiteralString {
        predicate: String,
    },
    PackageSpecifier {
        pattern: ModuleSpecifierPattern,
    },
}

impl IdentitySpec {
    /// Return a display-oriented string for the identity name.
    pub fn display_name(&self) -> String {
        match self {
            Self::Global { name } | Self::Heuristic { name } => name.to_string(),
            Self::ModuleExport { module, export } => format!("{module}.{export}"),
            Self::PackageModuleExport { module, export } => format!("{module}.{export}"),
            Self::ModuleNamespace { module } => module.to_string(),
            Self::PackageModuleNamespace { module } => module.to_string(),
            Self::Rooted { path } => path.to_string(),
            Self::LiteralString { predicate } => predicate.clone(),
            Self::PackageSpecifier { pattern } => pattern.to_string(),
        }
    }

    /// Stable diagnostic name for this identity kind.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Global { .. } => "global",
            Self::Heuristic { .. } => "heuristic",
            Self::ModuleExport { .. } => "module_export",
            Self::PackageModuleExport { .. } => "package_module_export",
            Self::ModuleNamespace { .. } => "module_namespace",
            Self::PackageModuleNamespace { .. } => "package_module_namespace",
            Self::Rooted { .. } => "rooted",
            Self::LiteralString { .. } => "literal",
            Self::PackageSpecifier { .. } => "package_specifier",
        }
    }
}

/// Declaration-owned event kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum EventSpec {
    Call,
    Construct,
    MemberCall { member: SymbolPath },
    MemberRead { member: SymbolPath },
    ClassReference,
    Import,
    StringReference,
}

impl EventSpec {
    /// Stable diagnostic name for this event kind.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Construct => "construct",
            Self::MemberCall { .. } => "member_call",
            Self::MemberRead { .. } => "member_read",
            Self::ClassReference => "class",
            Self::Import => "import",
            Self::StringReference => "string",
        }
    }
}

/// Declaration-owned subject relationship.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum SubjectSpec {
    /// The event is directly on the identity.
    Direct,
    /// The event is on an object returned from a producer.
    ReturnedFrom { producer: Box<IdentitySpec> },
    /// The event is on an instance created by a constructor.
    InstanceOf { constructor: Box<IdentitySpec> },
}

impl SubjectSpec {
    /// Stable diagnostic name for this subject relationship.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::ReturnedFrom { .. } => "returned_from",
            Self::InstanceOf { .. } => "instance_of",
        }
    }
}

// ── Typed logical query algebra ───────────────────────────────────────

/// Dense compiler variable ID assigned during query construction.
///
/// Variables are the main extensibility mechanism. Each variable has a semantic
/// type (event, object, identity, value) enforced by the predicates that bind
/// or constrain it. Variable names are authoring concerns and should not remain
/// in physical plans; runtime slots use dense validated IDs.
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

/// A single event selection with identity, subject, and argument constraints.
///
/// This is a leaf predicate — it selects an event occurrence and optionally
/// constrains its identity, subject relationship, and arguments. Evidence
/// metadata is not stored here; it lives in the owning [`EmissionDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventQuery {
    /// Variable bound by this event selection.
    pub var: VarId,
    /// Kind of event (call, construct, member call, etc.).
    pub event: EventSpec,
    /// Identity specification (global, rooted, module, etc.).
    pub identity: IdentitySpec,
    /// Subject relationship (direct, returned-from, instance-of).
    pub subject: SubjectSpec,
    /// Argument value constraints (empty for non-call events).
    pub constraints: Vec<ArgumentConstraint>,
}

/// A typed logical query expression.
///
/// The algebra supports four operators:
///
/// - `Event` — select a single event with identity, subject, and constraints;
/// - `Any` — union of independently complete alternatives;
/// - `All` — conjunction over predicates sharing explicitly compatible
///   variables; and
/// - `Lifecycle` — object lifecycle with source, condition, and completion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryExpr {
    /// A single event with identity, subject, and constraints.
    Event(EventQuery),
    /// Union of alternative expressions (must be non-empty).
    Any(AnyExpr),
    /// Conjunction of expressions sharing compatible variables (must be
    /// non-empty; correlation required for multi-event joins).
    All(AllExpr),
    /// Object lifecycle: source event, condition, and completion.
    Lifecycle(LifecycleQuery),
}

impl QueryExpr {
    /// Stable diagnostic name for this operator.
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Event(_) => "event",
            Self::Any(_) => "any",
            Self::All(_) => "all",
            Self::Lifecycle(_) => "lifecycle",
        }
    }

    /// Collect all variable IDs bound by this expression.
    pub fn vars(&self) -> Vec<VarId> {
        let mut ids = Vec::new();
        self.collect_vars(&mut ids);
        ids
    }

    fn collect_vars(&self, ids: &mut Vec<VarId>) {
        match self {
            Self::Event(q) => ids.push(q.var),
            Self::Any(a) => {
                for b in &a.branches {
                    b.collect_vars(ids);
                }
            }
            Self::All(a) => {
                for b in &a.branches {
                    b.collect_vars(ids);
                }
            }
            Self::Lifecycle(l) => ids.push(l.source.var),
        }
    }
}

impl fmt::Display for QueryExpr {
    /// Compact debug-oriented display showing the expression shape.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event(q) => {
                write!(
                    f,
                    "select {} {} {} {}",
                    q.var,
                    q.event.diagnostic_name(),
                    q.identity.diagnostic_name(),
                    q.subject.diagnostic_name()
                )
            }
            Self::Any(a) => {
                write!(f, "any [")?;
                for (i, b) in a.branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{b}")?;
                }
                write!(f, "]")
            }
            Self::All(a) => {
                write!(f, "all [")?;
                for (i, b) in a.branches.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{b}")?;
                }
                write!(f, "]")
            }
            Self::Lifecycle(l) => {
                write!(
                    f,
                    "lifecycle source={} condition={} completion={}",
                    l.source.var,
                    l.condition.is_some(),
                    l.completion.is_some()
                )
            }
        }
    }
}

/// Union of alternative query expressions.
///
/// Semantics:
/// - Normalize nested `Any`.
/// - Reject empty `Any`.
/// - Deduplicate equivalent branches.
/// - Preserve deterministic branch order.
/// - Merge duplicate results through the existing certainty/evidence policy.
/// - Do not let an unknown branch erase an independent complete witness.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnyExpr {
    pub branches: Vec<QueryExpr>,
}

impl AnyExpr {
    /// Create an `Any` expression. Returns an error if branches is empty.
    pub fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        if branches.is_empty() {
            return Err(QueryBuildError::EmptyAlternatives);
        }
        Ok(Self { branches })
    }
}

/// Conjunction of query expressions sharing compatible variables.
///
/// Semantics:
/// - Normalize nested `All`.
/// - Reject empty `All`.
/// - Attach single-event filters to the selecting scan where possible.
/// - Require explicit shared variables for multi-event joins (checked during
///   validation).
/// - Preserve one path-correlation token across all contributing predicates.
/// - Never combine evidence from incompatible alternatives.
/// - Propagate incomplete state without fabricating a witness.
/// - Select one explicit primary event for result emission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AllExpr {
    pub branches: Vec<QueryExpr>,
}

impl AllExpr {
    /// Create an `All` expression. Returns an error if branches is empty.
    pub fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        if branches.is_empty() {
            return Err(QueryBuildError::EmptyConjunction);
        }
        Ok(Self { branches })
    }
}

/// Object lifecycle: source event, condition, and completion.
///
/// Represents a bounded state machine tracking an object from its source
/// (production), through configuration (requirements), to completion (sink or
/// self-configuration). The compiler validates stages and compiles to the
/// existing local/cross-call flow engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleQuery {
    /// The event that produces the tracked object.
    pub source: EventQuery,
    /// Optional configuration condition (requirements).
    pub condition: Option<FlowCondition>,
    /// Optional completion mode (sink or configuration).
    pub completion: Option<FlowCompletion>,
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
    pub primary_var: VarId,
    /// Evidence kind (call, member call, flow, etc.).
    pub kind: MatchKind,
    /// Stable evidence symbol (e.g. "fetch", "document.createElement").
    pub symbol: String,
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
    pub expression: QueryExpr,
    /// How to emit evidence from the result.
    pub emission: EmissionDecl,
}

impl QueryDecl {
    /// Lower a [`MatcherDecl`] into a logical [`QueryDecl`].
    ///
    /// Each `MatcherDecl` becomes an `EventQuery` with a fresh variable bound
    /// by the event selection, plus an emission declaration referencing that
    /// variable. This is the canonical lowering path from the builder-style API
    /// to the logical algebra.
    pub fn from_matcher(decl: &MatcherDecl, var_id: VarId) -> Self {
        let expression = QueryExpr::Event(EventQuery {
            var: var_id,
            event: decl.event.clone(),
            identity: decl.identity.clone(),
            subject: decl.subject.clone(),
            constraints: decl.constraints.clone(),
        });
        let emission = EmissionDecl {
            primary_var: var_id,
            kind: decl.evidence_kind,
            symbol: decl.evidence_symbol.clone(),
        };
        Self {
            expression,
            emission,
        }
    }
}

impl fmt::Display for QueryDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "query emit={} {}", self.emission.symbol, self.expression)
    }
}

/// An ordered set of queries whose results are unioned.
///
/// A rule may have one or more independent queries. Each query produces its own
/// classified witnesses; the results are unioned across the set. This replaces
/// the implicit union of repeated `RuleBuilder::declaration` calls with an
/// explicit collection.
///
/// A `QuerySet` with a single query is the common case; multiple queries are
/// used when a rule matches several distinct event/identity combinations that
/// share metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuerySet {
    pub queries: Vec<QueryDecl>,
}

impl QuerySet {
    /// Create a query set from the given queries.
    pub fn new(queries: Vec<QueryDecl>) -> Self {
        Self { queries }
    }
}

impl fmt::Display for QuerySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QuerySet(")?;
        for (i, q) in self.queries.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{q}")?;
        }
        write!(f, ")")
    }
}

// ── Construction errors ──────────────────────────────────────────────

/// Errors from constructing or lowering logical query expressions.
///
/// These are declaration-layer errors, distinct from compiler validation
/// errors. They prevent construction of structurally invalid queries such as
/// empty `Any` or `All` expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryBuildError {
    /// An `Any` expression was constructed with zero branches.
    EmptyAlternatives,
    /// An `All` expression was constructed with zero branches.
    EmptyConjunction,
}

impl fmt::Display for QueryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAlternatives => write!(f, "Any expression must have at least one branch"),
            Self::EmptyConjunction => write!(f, "All expression must have at least one branch"),
        }
    }
}

// ── Plan summary for tests ───────────────────────────────────────────

/// A compact textual summary of a query plan, suitable for focused test
/// assertions. This is not a public schema and may change between releases.
///
/// Examples:
/// ```text
/// roots=1
/// event=1
/// ```
/// ```text
/// roots=2
/// event=2
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlanSummary {
    /// Number of query roots (declarations).
    pub roots: usize,
}

impl QueryPlanSummary {
    /// Compute a summary for an ordered set of queries.
    pub fn for_queries(queries: &[QueryDecl]) -> Self {
        Self {
            roots: queries.len(),
        }
    }
}

impl fmt::Display for QueryPlanSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "roots={}", self.roots)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        classification::MatchKind,
        rule::{MatcherDecl, ValueMatcher},
    };

    // ── VarId tests ────────────────────────────────────────────────

    #[test]
    fn var_id_new_round_trips() {
        let id = VarId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn var_id_ordering_is_stable() {
        let a = VarId::new(1);
        let b = VarId::new(2);
        assert!(a < b);
    }

    // ── AnyExpr / AllExpr empty rejection ──────────────────────────

    #[test]
    fn any_expr_rejects_empty_branches() {
        assert_eq!(
            AnyExpr::new(vec![]),
            Err(QueryBuildError::EmptyAlternatives)
        );
    }

    #[test]
    fn all_expr_rejects_empty_branches() {
        assert_eq!(AllExpr::new(vec![]), Err(QueryBuildError::EmptyConjunction));
    }

    #[test]
    fn any_expr_accepts_non_empty_branches() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = AnyExpr::new(vec![event.clone(), event]).unwrap();
        assert_eq!(any.branches.len(), 2);
    }

    #[test]
    fn all_expr_accepts_non_empty_branches() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let all = AllExpr::new(vec![event]).unwrap();
        assert_eq!(all.branches.len(), 1);
    }

    // ── Lowering: every builder method → valid QueryDecl ───────────

    /// Lower a MatcherDecl into a QueryDecl, checking basic invariants.
    fn lower_and_check(
        decl: Result<MatcherDecl, crate::api::rule::MatcherBuildError>,
    ) -> QueryDecl {
        let decl = decl.expect("valid matcher declaration");
        let q = QueryDecl::from_matcher(&decl, VarId::new(0));
        assert_eq!(q.emission.primary_var, VarId::new(0));
        assert_eq!(q.emission.symbol, decl.evidence_symbol);
        assert_eq!(q.emission.kind, decl.evidence_kind);
        match &q.expression {
            QueryExpr::Event(eq) => {
                assert_eq!(eq.var, VarId::new(0));
                assert_eq!(eq.event, decl.event);
                assert_eq!(eq.identity, decl.identity);
                assert_eq!(eq.subject, decl.subject);
                assert_eq!(eq.constraints, decl.constraints);
            }
            _ => panic!("expected Event expression"),
        }
        q
    }

    #[test]
    fn lowers_call_global_to_query_decl() {
        lower_and_check(MatcherDecl::builder().call_global("fetch").build());
    }

    #[test]
    fn lowers_call_heuristic_to_query_decl() {
        lower_and_check(MatcherDecl::builder().call_heuristic("fetch").build());
    }

    #[test]
    fn lowers_call_module_to_query_decl() {
        lower_and_check(MatcherDecl::builder().call_module("fs", "readFile").build());
    }

    #[test]
    fn lowers_call_package_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .call_package("@scope/pkg", "method")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_rooted_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_rooted("document.createElement")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_heuristic_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_heuristic("foo.bar")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_module_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_module("module", "method")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_instance_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_instance("pkg", "Client", "send")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_package_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_package("@scope/pkg", "method")
                .build(),
        );
    }

    #[test]
    fn lowers_member_call_returned_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_call_returned("create", "send")
                .build(),
        );
    }

    #[test]
    fn lowers_member_read_rooted_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_read_rooted("window.location")
                .build(),
        );
    }

    #[test]
    fn lowers_member_read_module_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_read_module("module", "property")
                .build(),
        );
    }

    #[test]
    fn lowers_member_read_returned_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_read_returned("create", "token")
                .build(),
        );
    }

    #[test]
    fn lowers_member_read_package_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .member_read_package("@scope/pkg", "property")
                .build(),
        );
    }

    #[test]
    fn lowers_import_exact_to_query_decl() {
        lower_and_check(MatcherDecl::builder().import_exact("node:fs").build());
    }

    #[test]
    fn lowers_import_package_to_query_decl() {
        lower_and_check(MatcherDecl::builder().import_package("@scope/pkg").build());
    }

    #[test]
    fn lowers_string_contains_to_query_decl() {
        lower_and_check(MatcherDecl::builder().string_contains("https://").build());
    }

    #[test]
    fn lowers_class_heuristic_to_query_decl() {
        lower_and_check(MatcherDecl::builder().class_heuristic("Worker").build());
    }

    #[test]
    fn lowers_class_module_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .class_module("module", "Klass")
                .build(),
        );
    }

    #[test]
    fn lowers_constructor_global_to_query_decl() {
        lower_and_check(MatcherDecl::builder().constructor_global("URL").build());
    }

    #[test]
    fn lowers_constructor_heuristic_to_query_decl() {
        lower_and_check(MatcherDecl::builder().constructor_heuristic("Foo").build());
    }

    #[test]
    fn lowers_constructor_module_to_query_decl() {
        lower_and_check(
            MatcherDecl::builder()
                .constructor_module("pkg", "Klass")
                .build(),
        );
    }

    #[test]
    fn lowers_arg_constraints_to_query_decl() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .arg(0, ValueMatcher::static_string())
            .arg_static_string(1)
            .arg_static_strings(2, ["a", "b"])
            .arg_static_string_contains(3, ["token"])
            .build()
            .expect("valid matcher with constraints");
        let q = QueryDecl::from_matcher(&decl, VarId::new(0));
        match &q.expression {
            QueryExpr::Event(eq) => {
                assert_eq!(eq.constraints.len(), 4);
            }
            _ => panic!("expected Event expression"),
        }
    }

    #[test]
    fn lowers_evidence_override_to_query_decl() {
        let decl = MatcherDecl::builder()
            .call_global("fetch")
            .evidence(MatchKind::CallArgument, "custom.fetch")
            .build()
            .expect("valid matcher with evidence override");
        let q = QueryDecl::from_matcher(&decl, VarId::new(0));
        assert_eq!(q.emission.kind, MatchKind::CallArgument);
        assert_eq!(q.emission.symbol, "custom.fetch");
    }

    // ── Equivalent builder forms produce equivalent lowering ──────

    #[test]
    fn semantically_equivalent_decls_lower_equally() {
        let decl_a = MatcherDecl::builder().call_global("fetch").build().unwrap();
        let decl_b = MatcherDecl::builder().call_global("fetch").build().unwrap();
        let q_a = QueryDecl::from_matcher(&decl_a, VarId::new(0));
        let q_b = QueryDecl::from_matcher(&decl_b, VarId::new(0));
        assert_eq!(q_a, q_b);
    }

    // ── Diagnostic names ──────────────────────────────────────────

    #[test]
    fn query_expr_diagnostic_names_are_stable() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        assert_eq!(event.diagnostic_name(), "event");

        let any = QueryExpr::Any(AnyExpr::new(vec![event.clone()]).unwrap());
        assert_eq!(any.diagnostic_name(), "any");

        let all = QueryExpr::All(AllExpr::new(vec![event]).unwrap());
        assert_eq!(all.diagnostic_name(), "all");
    }

    #[test]
    fn event_spec_diagnostic_names_are_stable() {
        assert_eq!(EventSpec::Call.diagnostic_name(), "call");
        assert_eq!(EventSpec::Construct.diagnostic_name(), "construct");
        assert_eq!(
            EventSpec::MemberCall {
                member: SymbolPath::from("foo")
            }
            .diagnostic_name(),
            "member_call"
        );
        assert_eq!(
            EventSpec::MemberRead {
                member: SymbolPath::from("foo")
            }
            .diagnostic_name(),
            "member_read"
        );
        assert_eq!(EventSpec::ClassReference.diagnostic_name(), "class");
        assert_eq!(EventSpec::Import.diagnostic_name(), "import");
        assert_eq!(EventSpec::StringReference.diagnostic_name(), "string");
    }

    #[test]
    fn identity_spec_diagnostic_names_are_stable() {
        assert_eq!(
            IdentitySpec::Global {
                name: SmolStr::new("f")
            }
            .diagnostic_name(),
            "global"
        );
        assert_eq!(
            IdentitySpec::Heuristic {
                name: SmolStr::new("f")
            }
            .diagnostic_name(),
            "heuristic"
        );
        assert_eq!(
            IdentitySpec::ModuleExport {
                module: SmolStr::new("m"),
                export: SmolStr::new("e")
            }
            .diagnostic_name(),
            "module_export"
        );
        assert_eq!(
            IdentitySpec::Rooted {
                path: SymbolPath::from("a.b")
            }
            .diagnostic_name(),
            "rooted"
        );
        assert_eq!(
            IdentitySpec::LiteralString {
                predicate: "s".into()
            }
            .diagnostic_name(),
            "literal"
        );
    }

    #[test]
    fn subject_spec_diagnostic_names_are_stable() {
        assert_eq!(SubjectSpec::Direct.diagnostic_name(), "direct");
        assert_eq!(
            SubjectSpec::ReturnedFrom {
                producer: Box::new(IdentitySpec::Global {
                    name: SmolStr::new("f")
                })
            }
            .diagnostic_name(),
            "returned_from"
        );
        assert_eq!(
            SubjectSpec::InstanceOf {
                constructor: Box::new(IdentitySpec::Global {
                    name: SmolStr::new("C")
                })
            }
            .diagnostic_name(),
            "instance_of"
        );
    }

    // ── Display and plan summary ──────────────────────────────────

    #[test]
    fn query_expr_display_shapes_are_compact() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let text = format!("{event}");
        assert!(text.contains("select"));
        assert!(text.contains("$0"));
        assert!(text.contains("call"));
        assert!(text.contains("global"));
        assert!(text.contains("direct"));
    }

    #[test]
    fn any_display_shows_branches() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = QueryExpr::Any(AnyExpr::new(vec![event]).unwrap());
        let text = format!("{any}");
        assert!(text.starts_with("any ["));
        assert!(text.ends_with(']'));
    }

    #[test]
    fn query_decl_display_includes_symbol() {
        let decl = MatcherDecl::builder().call_global("fetch").build().unwrap();
        let q = QueryDecl::from_matcher(&decl, VarId::new(0));
        let text = format!("{q}");
        assert!(text.contains("fetch"));
    }

    #[test]
    fn plan_summary_counts_roots() {
        let decls = [
            MatcherDecl::builder().call_global("fetch").build().unwrap(),
            MatcherDecl::builder()
                .member_read_rooted("window.location")
                .build()
                .unwrap(),
        ];
        let queries: Vec<QueryDecl> = decls
            .iter()
            .enumerate()
            .map(|(i, d)| {
                QueryDecl::from_matcher(
                    d,
                    VarId::new(u32::try_from(i).expect("test index fits in u32")),
                )
            })
            .collect();
        let summary = QueryPlanSummary::for_queries(&queries);
        assert_eq!(summary.roots, 2);
        assert_eq!(summary.to_string(), "roots=2");
    }

    // ── VarId collection ──────────────────────────────────────────

    #[test]
    fn event_query_vars_contains_one() {
        let event = QueryExpr::Event(EventQuery {
            var: VarId::new(5),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        assert_eq!(event.vars(), vec![VarId::new(5)]);
    }

    #[test]
    fn any_query_vars_collects_all_branch_vars() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("f"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(1),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("g"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let any = QueryExpr::Any(AnyExpr::new(vec![a, b]).unwrap());
        let vars = any.vars();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&VarId::new(0)));
        assert!(vars.contains(&VarId::new(1)));
    }
}
