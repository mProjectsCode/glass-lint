use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unshadowed global `fetch`, rooted `navigator.sendBeacon`, and the
/// global `XMLHttpRequest`, `WebSocket`, and `EventSource` constructors. Direct
/// aliases retain the corresponding browser-global provenance until
/// reassigned; local lookalikes and shadowed bindings are excluded. The rule
/// identifies request API use regardless of whether arguments are static or
/// dynamic and does not model other request libraries.
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
