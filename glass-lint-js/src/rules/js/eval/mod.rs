//! Dynamic-code evaluation rule definition.

use glass_lint_core::rules::{CallMatcher, Confidence, ConstructorMatcher, Rule, Severity};

/// Detects calls whose target is proven to be the global `eval` or `Function`
/// callable, plus construction through the global `Function`. Global-object
/// access, aliases, bind, call, and statically unpackable apply forms retain
/// callable identity; local, shadowed, reassigned, or mutated lookalikes do
/// not.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.eval")
        .label("Evaluates dynamic code")
        .category("language/dynamic-code")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(CallMatcher::global("eval"))
        .matcher(CallMatcher::global("Function"))
        .matcher(ConstructorMatcher::global("Function"))
        .build()
        .unwrap()
}
