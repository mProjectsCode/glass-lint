//! Obsidian vault read rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to vault `read`, `cachedRead`, and `readBinary`.
/// Provenance follows `this.app`, direct receiver aliases, static computed
/// properties, bounded rooted argument flow, source-ordered reassignment, and
/// lexical shadowing. Arguments and other read-like methods are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.read")
        .label("Reads vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.read"))
        .matcher(Matcher::rooted_member_call("app.vault.cachedRead"))
        .matcher(Matcher::rooted_member_call("app.vault.readBinary"))
        .build()
        .unwrap()
}
