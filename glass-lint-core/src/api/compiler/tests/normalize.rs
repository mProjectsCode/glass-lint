#[allow(unused_imports)]
use glass_lint_datastructures::SymbolPath;
#[allow(unused_imports)]
use smol_str::SmolStr;

#[allow(unused_imports)]
use crate::api::{
    compiler::{
        normalize::{self},
        normalized::{NormalizedQuery, NormalizedRoot},
        physical,
        requirements::{PlanRequirements, ProjectRequirement},
    },
    rule::{
        MatchKind, ValueMatcher,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec,
            LifecycleQuery, QueryDecl, QueryExpr, VarId,
        },
    },
};

fn event(var: u32, name: &str) -> QueryExpr {
    QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(var),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new(name),
        },
        vec![],
    ))
}

fn decl(expr: QueryExpr, primary_var: u32, symbol: &str) -> QueryDecl {
    QueryDecl::from_parts_for_test(
        expr,
        EmissionDecl {
            primary_var: VarId::new(primary_var),
            kind: MatchKind::Call,
            symbol: symbol.into(),
        },
    )
}

fn lifecycle(
    symbol: &str,
    sources: Vec<EventQuery>,
    condition: Option<crate::api::rule::LifecycleCondition>,
    completion: crate::api::rule::LifecycleCompletion,
) -> LifecycleQuery {
    LifecycleQuery::from_parts_for_test(symbol, sources, condition, completion)
}

fn normalize_ok(decl: &QueryDecl) -> NormalizedQuery {
    normalize::normalize_query_decl(decl).unwrap()
}

fn plan_requirements(query: &NormalizedQuery) -> PlanRequirements {
    physical::plan_normalized(query)
        .unwrap()
        .requirements()
        .clone()
}

mod algebra;
mod canonical;
mod pipeline;
