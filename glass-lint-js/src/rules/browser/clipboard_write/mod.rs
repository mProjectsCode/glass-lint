use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard write APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .label("Writes clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.clipboard.write"))
        .matcher(Matcher::rooted_member_call("navigator.clipboard.writeText"))
        .build()
        .unwrap()
}
