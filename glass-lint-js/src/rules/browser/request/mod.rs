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
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.sendBeacon")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("XMLHttpRequest")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("WebSocket")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("EventSource")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
