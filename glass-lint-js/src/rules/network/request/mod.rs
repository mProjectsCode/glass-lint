use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("network.request")
        .label("Uses browser network request APIs")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .matcher(Matcher::rooted_member_call("navigator.sendBeacon"))
        .matcher(Matcher::global_constructor("XMLHttpRequest"))
        .matcher(Matcher::global_constructor("WebSocket"))
        .matcher(Matcher::global_constructor("EventSource"))
        .build()
        .unwrap()
}
