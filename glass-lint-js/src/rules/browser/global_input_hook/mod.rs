use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects `document` or `window` event-listener registrations for the listed
/// keyboard and clipboard events. This is deliberately syntactic and therefore
/// reports same-shaped calls on shadowed local bindings; event names must
/// resolve to one of the configured static strings.
pub fn rule() -> Rule {
    Rule::builder("browser.global-input-hook")
        .label("Registers global input handlers")
        .category("browser/input")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("document.addEventListener")
                .arg_string(0, ["keydown", "keyup", "paste", "copy", "cut"]),
        ))
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("window.addEventListener")
                .arg_string(0, ["keydown", "keyup", "paste", "copy", "cut"]),
        ))
        .build()
        .unwrap()
}
