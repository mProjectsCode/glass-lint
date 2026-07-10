use glass_lint_core::rules::{CallMatcher, Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects unshadowed global `setTimeout` calls with a static string first
/// argument, plus syntactic `window.setInterval` and `globalThis.setTimeout`
/// equivalents. Function callbacks and dynamic strings are excluded; the two
/// rooted-looking member chains intentionally remain heuristic lookalikes.
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
