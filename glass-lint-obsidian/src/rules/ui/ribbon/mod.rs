//! Obsidian ribbon-registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic `this.addRibbonIcon()` registration call, including
/// statically computed property names. The instance matcher requires a proven
/// Obsidian `Plugin` receiver and does not follow aliases or reassignment;
/// other receivers, dynamic properties, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("ui.ribbon")
        .description("Registers ribbon icons")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "addRibbonIcon",
        ))
        .build()
        .unwrap()
}
