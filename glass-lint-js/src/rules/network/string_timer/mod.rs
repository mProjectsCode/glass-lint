use glass_lint_core::rules::{CallMatcher, Confidence, Matcher, MemberCallMatcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("dynamic-code.string-timer")
        .label("Runs code from a string timer")
        .category("language/dynamic-code")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::call(
            CallMatcher::global("setTimeout").static_string_arg(0),
        ))
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("window.setInterval").static_string_arg(0),
        ))
        .matcher(Matcher::member_call(
            MemberCallMatcher::syntactic_heuristic("globalThis.setTimeout").static_string_arg(0),
        ))
        .build()
        .unwrap()
}
