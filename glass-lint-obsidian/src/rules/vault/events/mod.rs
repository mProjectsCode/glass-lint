use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted registration through `app.vault.on`, including `this.app`,
/// direct receiver aliases, and static computed properties. Source-ordered
/// reassignment and lexical shadowing are respected; event names, handlers,
/// arguments, and other event methods are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.events")
        .label("Registers vault events")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.on"))
        .build()
        .unwrap()
}
