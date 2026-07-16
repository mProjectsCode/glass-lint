//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects imports of the `crypto`, `node:crypto`, and `crypto-js` modules,
/// plus syntactic `crypto.subtle` digest/encrypt/decrypt calls. Import reports
/// are intentionally emitted at the import rather than later API use; the
/// heuristic Web Crypto chains can match same-shaped local bindings.
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .label("Uses cryptographic operations")
        .category("language/crypto")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("crypto"))
        .matcher(Matcher::import("node:crypto"))
        .matcher(Matcher::import("crypto-js"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.digest"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.encrypt"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.decrypt"))
        .build()
        .unwrap()
}
