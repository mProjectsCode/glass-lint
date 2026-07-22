use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls proven to target global `fetch`, rooted
/// `navigator.sendBeacon`, and the global `XMLHttpRequest`, `WebSocket`, and
/// `EventSource` constructors. Global-object access and direct aliases retain
/// browser-global provenance until reassigned; local, shadowed, or mutated
/// lookalikes are excluded. The rule identifies request API use regardless of
/// whether arguments are static or dynamic and does not model other request
/// libraries.
pub fn rule() -> Rule {
    Rule::builder("network.request")
        .description("Uses browser network request APIs")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::global_call("fetch"))
        .declaration(MatcherDecl::rooted_member_call("navigator.sendBeacon"))
        .declaration(MatcherDecl::global_constructor("XMLHttpRequest"))
        .declaration(MatcherDecl::global_constructor("WebSocket"))
        .declaration(MatcherDecl::global_constructor("EventSource"))
        .build()
        .unwrap()
}
