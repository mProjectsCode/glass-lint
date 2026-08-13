//! Obsidian view-registration rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic `this.registerView()` call, including a statically
/// computed property name. The instance matcher requires a proven Obsidian
/// `Plugin` receiver and follows proven receiver and callable aliases;
/// reassignment, other receivers, dynamic properties, and near-name methods are
/// excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("view.register")
        .description("Registers Obsidian views")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "registerView",
        ))
        .build()
        .unwrap()
}
