use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects calls proven to target global `fetch`, rooted
/// `navigator.sendBeacon`, and the global `XMLHttpRequest`, `WebSocket`, and
/// `EventSource` constructors. Global-object access and direct aliases retain
/// browser-global provenance until reassigned; local, shadowed, or mutated
/// lookalikes are excluded. The rule identifies request API use regardless of
/// whether arguments are static or dynamic and does not model other request
/// libraries.
pub fn rule() -> Rule {
    Rule::catalog_builder("network.request")
        .description("Uses browser network request APIs")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .query(EventQuery::member_call_rooted("navigator.sendBeacon"))
        .query(EventQuery::constructor_global("XMLHttpRequest"))
        .query(EventQuery::constructor_global("WebSocket"))
        .query(EventQuery::constructor_global("EventSource"))
        .build()
        .unwrap()
}
