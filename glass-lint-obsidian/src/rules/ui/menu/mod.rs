//! Obsidian menu rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects proven `obsidian.Menu` instance calls. Proven instance aliases are
/// retained; unproven callback parameters and same-shaped local receivers are
/// excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("ui.menu")
        .description("Uses Obsidian menus")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_call_instance(
            "obsidian", "Menu", "addItem",
        ))
        .build()
        .unwrap()
}
