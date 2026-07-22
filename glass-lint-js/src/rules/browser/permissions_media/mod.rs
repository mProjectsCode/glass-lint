//! Browser media-capture permission rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects unshadowed `navigator.mediaDevices.getUserMedia` and
/// `getDisplayMedia` calls and aliases
/// derived from that browser API. Locally shadowed `navigator` bindings and
/// aliases that are later reassigned do not retain browser provenance.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-media")
        .description("Uses browser media capture")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.mediaDevices.getUserMedia",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.mediaDevices.getDisplayMedia",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.mediaDevices.enumerateDevices",
        ))
        .build()
        .unwrap()
}
