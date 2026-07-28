//! Obsidian vault read rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects rooted calls to vault `read`, `cachedRead`, and `readBinary`.
/// Provenance follows `this.app`, direct receiver aliases, static computed
/// properties, bounded rooted argument flow, source-ordered reassignment, and
/// lexical shadowing. Arguments and other read-like methods are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("vault.read")
        .description("Reads vault files")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("app.vault.read"))
        .query(QueryDecl::member_call_rooted("app.vault.cachedRead"))
        .query(QueryDecl::member_call_rooted("app.vault.readBinary"))
        .build()
        .unwrap()
}
