//! Dynamic-code evaluation rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects calls whose target is proven to be the global `eval` or `Function`
/// callable, plus construction through the global `Function`. Global-object
/// access, aliases, bind, call, and statically unpackable apply forms retain
/// callable identity; local, shadowed, reassigned, or mutated lookalikes do
/// not.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.eval")
        .description("Evaluates dynamic code")
        .category(Category::new("language/dynamic-code").unwrap())
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .query(QueryDecl::call_global("eval"))
        .query(QueryDecl::call_global("Function"))
        .query(QueryDecl::constructor_global("Function"))
        .build()
        .unwrap()
}
