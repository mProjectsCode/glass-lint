use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to vault and adapter `getResourcePath`, plus literal
/// or static-template fragments containing `obsidian://`. Rooted provenance
/// follows `this.app`, direct receiver aliases, static computed properties,
/// source-ordered reassignment, and lexical shadowing; the URL marker remains
/// a raw literal heuristic. Arguments, dynamic string reconstruction, and
/// other URL schemes are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.resource-url")
        .label("Accesses attachment resource paths")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.getResourcePath"))
        .matcher(Matcher::rooted_member_call(
            "app.vault.adapter.getResourcePath",
        ))
        .matcher(Matcher::string_literal("obsidian://"))
        .build()
        .unwrap()
}
