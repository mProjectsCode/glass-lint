//! Browser persistent-storage rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the listed unshadowed browser storage calls and aliases derived
/// from them: `getItem`/`setItem` on local and session storage,
/// `indexedDB.open`, and `caches.open`. Other storage methods, shadowed
/// globals, and reassigned aliases are outside this rule's scope.
pub fn rule() -> Rule {
    Rule::builder("browser.persistent-storage")
        .description("Uses persistent browser storage")
        .category("browser/storage")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("localStorage.getItem"))
        .matcher(Matcher::rooted_member_call("localStorage.setItem"))
        .matcher(Matcher::rooted_member_call("sessionStorage.getItem"))
        .matcher(Matcher::rooted_member_call("sessionStorage.setItem"))
        .matcher(Matcher::rooted_member_call("indexedDB.open"))
        .matcher(Matcher::rooted_member_call("caches.open"))
        .build()
        .unwrap()
}
