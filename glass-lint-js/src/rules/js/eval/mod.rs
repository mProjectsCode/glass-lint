//! Dynamic-code evaluation rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .call_global("eval")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .call_global("Function")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("Function")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
