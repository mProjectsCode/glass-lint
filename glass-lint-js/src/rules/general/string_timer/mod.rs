//! String-based timer dynamic-code rule definition.

use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

/// Detects calls proven to target the global `setTimeout` or `setInterval`
/// callable with a static string first argument. Global-object access and
/// callable aliases retain identity; local, shadowed, reassigned, function,
/// and dynamic-value lookalikes are excluded.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.string-timer")
        .label("Runs code from a string timer")
        .category("language/dynamic-code")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(CallMatcher::global("setTimeout").static_string_arg(0))
        .matcher(CallMatcher::global("setInterval").static_string_arg(0))
        .build()
        .unwrap()
}
