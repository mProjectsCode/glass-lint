//! Typed logical query expressions and composition operators.

use std::fmt;

use super::{
    EventQuery, EventSelection, LifecycleQuery, QueryBuildError, QueryPredicate, VarId, limits,
};

/// A typed logical query expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryExpr {
    pub(crate) kind: QueryExprKind,
}

/// Internal expression kind used by the compiler.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum QueryExprKind {
    Event(EventQuery),
    SelectEvent(EventSelection),
    Require(QueryPredicate),
    Any(AnyExpr),
    All(AllExpr),
    Lifecycle(LifecycleQuery),
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
            kind: QueryExprKind::SelectEvent(EventSelection { bind }),
        }
    }

    pub(crate) fn require(predicate: QueryPredicate) -> Self {
        Self {
            kind: QueryExprKind::Require(predicate),
        }
    }

    fn depth(&self) -> usize {
        match &self.kind {
            QueryExprKind::Any(any) => 1 + any.branches.iter().map(Self::depth).max().unwrap_or(0),
            QueryExprKind::All(all) => 1 + all.branches.iter().map(Self::depth).max().unwrap_or(0),
            QueryExprKind::Event(_)
            | QueryExprKind::SelectEvent(_)
            | QueryExprKind::Require(_)
            | QueryExprKind::Lifecycle(_) => 1,
        }
    }

    #[allow(dead_code)]
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

    pub fn vars(&self) -> Vec<VarId> {
        let mut ids = Vec::new();
        self.collect_vars(&mut ids);
        ids
    }

    fn collect_vars(&self, ids: &mut Vec<VarId>) {
        match &self.kind {
            QueryExprKind::Event(q) => ids.push(q.var),
            QueryExprKind::SelectEvent(selection) => ids.push(selection.bind),
            QueryExprKind::Require(predicate) => match predicate {
                QueryPredicate::EventKind { event, .. }
                | QueryPredicate::EventIdentity { event, .. } => ids.push(*event),
                QueryPredicate::Argument { call, .. } => ids.push(*call),
                QueryPredicate::ReturnedObject { bind, .. }
                | QueryPredicate::ConstructedObject { bind, .. } => ids.push(*bind),
                QueryPredicate::MemberSubject { event, object } => {
                    ids.push(*event);
                    ids.push(*object);
                }
            },
            QueryExprKind::Any(any) => any
                .branches
                .iter()
                .for_each(|branch| branch.collect_vars(ids)),
            QueryExprKind::All(all) => all
                .branches
                .iter()
                .for_each(|branch| branch.collect_vars(ids)),
            QueryExprKind::Lifecycle(lifecycle) => {
                lifecycle
                    .sources
                    .iter()
                    .for_each(|source| ids.push(source.var));
            }
        }
    }
}

impl fmt::Display for QueryExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            QueryExprKind::Event(q) => write!(
                f,
                "select {} {} {}",
                q.var,
                q.event.diagnostic_name(),
                q.identity.diagnostic_name()
            ),
            QueryExprKind::SelectEvent(selection) => write!(f, "bind {}", selection.bind),
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
            QueryExprKind::Any(any) => write_list(f, "any", &any.branches),
            QueryExprKind::All(all) => write_list(f, "all", &all.branches),
            QueryExprKind::Lifecycle(lifecycle) => write!(
                f,
                "lifecycle sources={} condition={} completion={}",
                lifecycle.sources.len(),
                lifecycle.condition.is_some(),
                lifecycle.completion.is_some()
            ),
        }
    }
}

fn write_list(f: &mut fmt::Formatter<'_>, name: &str, branches: &[QueryExpr]) -> fmt::Result {
    write!(f, "{name} [")?;
    for (index, branch) in branches.iter().enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{branch}")?;
    }
    write!(f, "]")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnyExpr {
    pub(crate) branches: Vec<QueryExpr>,
}

impl AnyExpr {
    pub(crate) fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        validate_children(
            &branches,
            "Any expression branches",
            QueryBuildError::EmptyAlternatives,
        )?;
        Ok(Self { branches })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AllExpr {
    pub(crate) branches: Vec<QueryExpr>,
}

impl AllExpr {
    pub(crate) fn new(branches: Vec<QueryExpr>) -> Result<Self, QueryBuildError> {
        validate_children(
            &branches,
            "All expression branches",
            QueryBuildError::EmptyConjunction,
        )?;
        Ok(Self { branches })
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
