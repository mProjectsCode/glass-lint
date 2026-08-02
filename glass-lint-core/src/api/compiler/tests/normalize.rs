#[allow(unused_imports)]
use glass_lint_datastructures::SymbolPath;
#[allow(unused_imports)]
use smol_str::SmolStr;

#[allow(unused_imports)]
use crate::api::{
    classification::MatchKind,
    compiler::{
        normalize::{self},
        normalized::{NormalizedQuery, NormalizedRoot},
        requirements::{PlanRequirements, ProjectRequirement, ValueResolutionRequirement},
    },
    rule::{
        ValueMatcher,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec,
            LifecycleQuery, QueryDecl, QueryExpr, VarId,
        },
    },
};

fn event(var: u32, name: &str) -> QueryExpr {
    QueryExpr::event(EventQuery {
        var: VarId::new(var),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new(name),
        },
        constraints: vec![],
    })
}

fn decl(expr: QueryExpr, primary_var: u32, symbol: &str) -> QueryDecl {
    QueryDecl {
        expression: expr,
        emission: EmissionDecl {
            primary_var: VarId::new(primary_var),
            kind: MatchKind::Call,
            symbol: symbol.into(),
        },
    }
}

fn lifecycle(
    symbol: &str,
    sources: Vec<EventQuery>,
    condition: Option<crate::api::rule::LifecycleCondition>,
    completion: Option<crate::api::rule::LifecycleCompletion>,
) -> LifecycleQuery {
    LifecycleQuery {
        symbol: symbol.into(),
        sources,
        condition,
        completion,
    }
}

fn normalize_ok(decl: &QueryDecl) -> NormalizedQuery {
    normalize::normalize_query_decl(decl).unwrap()
}

mod algebra;
mod canonical;
mod pipeline;
