//! Browser executable-script-injection rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects `document.createElement` calls whose tag argument resolves to
/// `script`, including constant string concatenation and aliases of
/// `createElement`. It reports creation itself, without requiring insertion or
/// executable content, and does not prove that `document` is the browser
/// global; dynamic values and other static tags are excluded.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.script-injection")
        .description("Injects executable script elements")
        .category("browser/dom")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::from(
            MemberCallMatcher::heuristic("document.createElement")
                .arg_static_strings(0, ["script"]),
        ))
        .build()
        .unwrap()
}
