//! Obsidian notice rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects the exact global `Notice` constructor plus constructors and
/// subclasses proven to come from the `obsidian` module. Local/shadowed and
/// reassigned names are excluded, while global-object, ESM, namespace, and
/// CommonJS provenance is followed. Constructor arguments and subclass bodies
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("ui.notice")
        .description("Uses Obsidian notices")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::global_constructor("Notice"))
        .declaration(MatcherDecl::module_constructor("obsidian", "Notice"))
        .declaration(MatcherDecl::module_class("obsidian", "Notice"))
        .build()
        .unwrap()
}
