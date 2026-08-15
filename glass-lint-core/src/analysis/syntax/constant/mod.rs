pub(super) mod eval;
#[cfg(test)]
mod tests;
mod types;

pub(in crate::analysis) use eval::{
    EvalNode, EvalState, Lookup, NoLookup, contextual_member_property_name, evaluate,
};
#[cfg(test)]
pub(in crate::analysis) use types::MAX_ARRAY_ITEMS;
pub(in crate::analysis) use types::{ConstValue, MAX_OBJECT_KEYS, non_negative_integer};

/// Evaluate an expression as one bounded static string.
pub(in crate::analysis) fn static_string(
    expr: &swc_ecma_ast::Expr,
    lookup: &impl Lookup,
) -> Option<String> {
    evaluate(expr, lookup).string().map(str::to_owned)
}
