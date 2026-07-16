//! Browser network-request rule definition.

use glass_lint_core::rules::{
    CallMatcher, Confidence, ConstructorMatcher, MemberCallMatcher, Rule, Severity,
};

/// Detects calls proven to target global `fetch`, rooted
/// `navigator.sendBeacon`, and the global `XMLHttpRequest`, `WebSocket`, and
/// `EventSource` constructors. Global-object access and direct aliases retain
/// browser-global provenance until reassigned; local, shadowed, or mutated
/// lookalikes are excluded. The rule identifies request API use regardless of
/// whether arguments are static or dynamic and does not model other request
/// libraries.
pub fn rule() -> Rule {
    Rule::builder("network.request")
        .label("Uses browser network request APIs")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(CallMatcher::global("fetch"))
        .matcher(MemberCallMatcher::rooted("navigator.sendBeacon"))
        .matcher(ConstructorMatcher::global("XMLHttpRequest"))
        .matcher(ConstructorMatcher::global("WebSocket"))
        .matcher(ConstructorMatcher::global("EventSource"))
        .build()
        .unwrap()
}
