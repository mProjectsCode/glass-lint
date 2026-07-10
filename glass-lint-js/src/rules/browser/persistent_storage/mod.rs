use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.persistent-storage")
        .label("Uses persistent browser storage")
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
