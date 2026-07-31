pub(super) mod eval;
#[cfg(test)]
mod tests;
mod types;

pub(in crate::analysis) use eval::{EvalState, Lookup, NoLookup, evaluate, property_name};
pub(in crate::analysis) use types::{ConstValue, non_negative_integer};

/// Evaluate an expression as one bounded static string.
pub(in crate::analysis) fn static_string(
    expr: &swc_ecma_ast::Expr,
    lookup: &impl Lookup,
) -> Option<String> {
    evaluate(expr, lookup).string().map(str::to_owned)
}
