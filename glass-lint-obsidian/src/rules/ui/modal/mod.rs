//! Obsidian modal rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects `Modal` constructors and subclass expressions proven to originate
/// from the `obsidian` module through ESM, CommonJS, or namespace aliases.
/// Local, unbound, shadowed, and reassigned names are excluded; constructor
/// arguments and class bodies are ignored.
pub fn rule() -> Rule {
    Rule::builder("ui.modal")
        .description("Uses Obsidian modal UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::module_constructor("obsidian", "Modal"))
        .declaration(MatcherDecl::module_class("obsidian", "Modal"))
        .build()
        .unwrap()
}
