#[allow(unused_imports)]
use glass_lint_datastructures::SymbolPath;
#[allow(unused_imports)]
use smol_str::SmolStr;

#[allow(unused_imports)]
use crate::api::{
    compiler::validate::{
        QueryCompileError, pass_correlation_evidence, pass_scope_types, pass_structure,
        validate_query_decl,
    },
    rule::{
        ArgumentConstraint, MatchKind, QueryDecl, ValueMatcher,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery,
            QueryExpr, QueryPredicate, VarId,
        },
    },
};

fn assert_valid_query(decl: &QueryDecl) {
    if let Err(e) = validate_query_decl(decl) {
        panic!("query validation failed: {} ({})", e, e.diagnostic_name());
    }
}

/// Build the common direct global-call expression used by validation cases.
fn global_call(var: u32, name: impl Into<SmolStr>) -> QueryExpr {
    QueryExpr::event(EventQuery::from_parts_for_test(
        VarId::new(var),
        EventSpec::Call,
        IdentitySpec::Global { name: name.into() },
        vec![],
    ))
}

/// Add the explicit emission defaults shared by direct query fixtures.
fn emitted(
    expression: QueryExpr,
    primary_var: u32,
    kind: MatchKind,
    symbol: impl Into<String>,
) -> QueryDecl {
    QueryDecl::from_parts_for_test(
        expression,
        EmissionDecl {
            primary_var: VarId::new(primary_var),
            kind,
            symbol: symbol.into(),
        },
    )
}

mod correlation;
mod diagnostics;
mod identity;
mod well_formedness;
