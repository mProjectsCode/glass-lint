//! String-based timer dynamic-code rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects calls proven to target the HTML timer globals `setTimeout` or
/// `setInterval` with a static string first argument. Global-object access and
/// callable aliases retain identity; local, shadowed, reassigned, function,
/// and dynamic-value lookalikes are excluded. DOM event attributes and other
/// callback-only scheduling APIs are outside this rule.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.string-timer")
        .description("Runs code from an HTML string timer")
        .category(Category::new("language/dynamic-code").unwrap())
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .query(
            EventQuery::call_global("setTimeout")
                .map(|q| q.with_arg_static_string(0).unwrap().into_query())
                .unwrap(),
        )
        .query(
            EventQuery::call_global("setInterval")
                .map(|q| q.with_arg_static_string(0).unwrap().into_query())
                .unwrap(),
        )
        .build()
        .unwrap()
}
