//! Browser global-input listener rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects `document` or `window` event-listener registrations for the listed
/// keyboard and clipboard events. This is deliberately syntactic and therefore
/// reports same-shaped calls on shadowed local bindings; event names must
/// resolve to one of the configured static strings.
pub fn rule() -> Rule {
    Rule::builder("browser.global-input-hook")
        .description("Registers global input handlers")
        .category("browser/input")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::from(
            MemberCallMatcher::heuristic("document.addEventListener")
                .arg_static_strings(0, ["keydown", "keyup", "paste", "copy", "cut"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::heuristic("window.addEventListener")
                .arg_static_strings(0, ["keydown", "keyup", "paste", "copy", "cut"]),
        ))
        .build()
        .unwrap()
}
