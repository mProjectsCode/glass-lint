//! String-based timer dynamic-code rule definition.

use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

/// Detects calls proven to target the HTML timer globals `setTimeout` or
/// `setInterval` with a static string first argument. Global-object access and
/// callable aliases retain identity; local, shadowed, reassigned, function,
/// and dynamic-value lookalikes are excluded. DOM event attributes and other
/// callback-only scheduling APIs are outside this rule.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.string-timer")
        .description("Runs code from an HTML string timer")
        .category("language/dynamic-code")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(CallMatcher::global("setTimeout").arg_static_string(0))
        .matcher(CallMatcher::global("setInterval").arg_static_string(0))
        .build()
        .unwrap()
}
