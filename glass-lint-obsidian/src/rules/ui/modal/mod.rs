//! Obsidian modal rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects `Modal` constructors and subclass expressions proven to originate
/// from the `obsidian` module through ESM, CommonJS, or namespace aliases.
/// Local, unbound, shadowed, and reassigned names are excluded; constructor
/// arguments and class bodies are ignored.
pub fn rule() -> Rule {
    Rule::builder("ui.modal")
        .description("Uses Obsidian modal UI")
        .category(Category::new("ui").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::constructor_module("obsidian", "Modal"))
        .query(EventQuery::class_module("obsidian", "Modal"))
        .build()
        .unwrap()
}
