//! Typed logical query expressions and composition operators.

use std::fmt;

use crate::api::rule::query::{
    EventQuery, LifecycleQuery, QueryBuildError, QueryPredicate, VarId, limits,
};

/// A typed logical query expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryExpr {
    kind: QueryExprKind,
}

/// Internal expression kind used by the compiler.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum QueryExprKind {
    Event(EventQuery),
    SelectEvent(VarId),
    Require(QueryPredicate),
    Any(AnyExpr),
    All(AllExpr),
    Lifecycle(LifecycleQuery),
}

/// The role of a variable in an expression node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VarRole {
    Binding,
    Reference,
}

impl QueryExpr {
    pub(crate) fn event(eq: EventQuery) -> Self {
        Self {
            kind: QueryExprKind::Event(eq),
        }
    }

    pub(crate) fn any(any: AnyExpr) -> Self {
        Self {
            kind: QueryExprKind::Any(any),
        }
    }

    pub(crate) fn all(all: AllExpr) -> Self {
        Self {
            kind: QueryExprKind::All(all),
        }
    }

    pub(crate) fn lifecycle(lifecycle: LifecycleQuery) -> Self {
        Self {
            kind: QueryExprKind::Lifecycle(lifecycle),
        }
    }

    pub(crate) fn select_event(bind: VarId) -> Self {
        Self {
            kind: QueryExprKind::SelectEvent(bind),
        }
    }

    pub(crate) fn require(predicate: QueryPredicate) -> Self {
        Self {
            kind: QueryExprKind::Require(predicate),
        }
    }

    fn depth(&self) -> usize {
        match &self.kind {
            QueryExprKind::Any(any) => 1 + any.iter().map(Self::depth).max().unwrap_or(0),
            QueryExprKind::All(all) => 1 + all.iter().map(Self::depth).max().unwrap_or(0),
            QueryExprKind::Event(_)
            | QueryExprKind::SelectEvent(_)
            | QueryExprKind::Require(_)
            | QueryExprKind::Lifecycle(_) => 1,
        }
    }

    pub(crate) fn kind(&self) -> &QueryExprKind {
        &self.kind
    }

    pub fn diagnostic_name(&self) -> &'static str {
        match &self.kind {
            QueryExprKind::Event(_) => "event",
            QueryExprKind::SelectEvent(_) => "select_event",
            QueryExprKind::Require(_) => "require",
            QueryExprKind::Any(_) => "any",
            QueryExprKind::All(_) => "all",
            QueryExprKind::Lifecycle(_) => "lifecycle",
        }
    }

    /// Walk all variables in the expression, calling `f` for each with its
    /// role.
    pub(crate) fn walk_vars(&self, f: &mut impl FnMut(VarId, VarRole)) {
        self.walk_vars_until(&mut |id, role| {
            f(id, role);
            false
        });
    }

    pub(crate) fn shape_facts(&self) -> QueryShapeFacts {
        let mut facts = QueryShapeFacts::default();
        self.walk_vars(&mut |id, role| {
            facts.variables.push(id);
            if role == VarRole::Binding {
                facts.bindings.push(id);
            }
        });
        facts
    }

    fn walk_vars_until(&self, f: &mut impl FnMut(VarId, VarRole) -> bool) -> bool {
        match &self.kind {
            QueryExprKind::Event(q) => f(q.var(), VarRole::Binding),
            QueryExprKind::SelectEvent(bind) => f(*bind, VarRole::Binding),
            QueryExprKind::Require(p) => match p {
                QueryPredicate::EventKind { event, .. }
                | QueryPredicate::EventIdentity { event, .. } => f(*event, VarRole::Reference),
                QueryPredicate::Argument { call, .. } => f(*call, VarRole::Reference),
                QueryPredicate::ReturnedObject { bind, .. }
                | QueryPredicate::ConstructedObject { bind, .. } => f(*bind, VarRole::Binding),
                QueryPredicate::MemberSubject { event, object } => {
                    f(*event, VarRole::Reference) || f(*object, VarRole::Reference)
                }
            },
            QueryExprKind::Any(any) => any.iter().any(|b| b.walk_vars_until(f)),
            QueryExprKind::All(all) => all.iter().any(|b| b.walk_vars_until(f)),
            QueryExprKind::Lifecycle(lc) => lc
                .sources()
                .iter()
                .any(|src| f(src.var(), VarRole::Binding)),
        }
    }

    #[cfg(test)]
    pub(crate) fn vars(&self) -> Vec<VarId> {
        self.shape_facts().variables
    }

    pub(crate) fn contains_var(&self, target: VarId) -> bool {
        self.walk_vars_until(&mut |id, _| id == target)
    }
}

#[derive(Debug, Default)]
pub(crate) struct QueryShapeFacts {
    variables: Vec<VarId>,
    bindings: Vec<VarId>,
}

impl QueryShapeFacts {
    pub(crate) fn variables(&self) -> &[VarId] {
        &self.variables
    }

    pub(crate) fn bindings(&self) -> &[VarId] {
        &self.bindings
    }
}

impl fmt::Display for QueryExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            QueryExprKind::Event(q) => write!(
                f,
                "select {} {} {}",
                q.var(),
                q.event().diagnostic_name(),
                q.identity().diagnostic_name()
            ),
            QueryExprKind::SelectEvent(bind) => write!(f, "bind {bind}"),
            QueryExprKind::Require(predicate) => match predicate {
                QueryPredicate::EventKind { event, expected } => {
                    write!(f, "kind({event})={}", expected.diagnostic_name())
                }
                QueryPredicate::EventIdentity { event, expected } => {
                    write!(f, "id({event})={}", expected.diagnostic_name())
                }
                QueryPredicate::Argument { call, index, .. } => {
                    write!(f, "arg({call})[{}]", index.get())
                }
                QueryPredicate::ReturnedObject { bind, .. } => write!(f, "returned({bind})"),
                QueryPredicate::ConstructedObject { bind, .. } => write!(f, "constructed({bind})"),
                QueryPredicate::MemberSubject { event, object } => {
                    write!(f, "member({event},{object})")
                }
            },
            QueryExprKind::Any(any) => write_list(f, "any", any.iter()),
            QueryExprKind::All(all) => write_list(f, "all", all.iter()),
            QueryExprKind::Lifecycle(lifecycle) => write!(
                f,
                "lifecycle sources={} condition={}",
                lifecycle.sources().len(),
                lifecycle.condition().is_some(),
            ),
        }
    }
}

fn write_list<'a>(
    f: &mut fmt::Formatter<'_>,
    name: &str,
    branches: impl Iterator<Item = &'a QueryExpr>,
) -> fmt::Result {
    write!(f, "{name} [")?;
    for (index, branch) in branches.enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{branch}")?;
    }
    write!(f, "]")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LogicalBranches(Vec<QueryExpr>);

impl LogicalBranches {
    fn new(
        branches: Vec<QueryExpr>,
        label: &'static str,
        empty: QueryBuildError,
    ) -> Result<Self, QueryBuildError> {
        validate_children(&branches, label, empty)?;
        Ok(Self(branches))
    }

    fn iter(&self) -> impl Iterator<Item = &QueryExpr> {
        self.0.iter()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnyExpr {
    branches: LogicalBranches,
}

impl AnyExpr {
    pub(crate) fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        Ok(Self {
            branches: LogicalBranches::new(
                branches,
                "Any expression branches",
                QueryBuildError::EmptyAlternatives,
            )?,
        })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &QueryExpr> {
        self.branches.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.branches.len()
    }

    pub(crate) fn all_branches_contain(&self, target: VarId) -> bool {
        self.iter().all(|branch| branch.contains_var(target))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AllExpr {
    branches: LogicalBranches,
}

impl AllExpr {
    pub(crate) fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        Ok(Self {
            branches: LogicalBranches::new(
                branches,
                "All expression branches",
                QueryBuildError::EmptyConjunction,
            )?,
        })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &QueryExpr> {
        self.branches.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.branches.len()
    }
}

fn validate_children(
    branches: &[QueryExpr],
    label: &'static str,
    empty: QueryBuildError,
) -> Result<(), QueryBuildError> {
    if branches.is_empty() {
        return Err(empty);
    }
    if branches.len() > limits::MAX_EXPR_CHILDREN {
        return Err(QueryBuildError::CollectionTooLarge(label, branches.len()));
    }
    let depth = 1 + branches.iter().map(QueryExpr::depth).max().unwrap_or(0);
    if depth > limits::MAX_EXPR_DEPTH {
        return Err(QueryBuildError::ExpressionDepthExceeded(depth));
    }
    Ok(())
}
