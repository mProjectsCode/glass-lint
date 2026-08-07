//! Obsidian notice rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects the exact global `Notice` constructor plus constructors and
/// subclasses proven to come from the `obsidian` module. Local/shadowed and
/// reassigned names are excluded, while global-object, ESM, namespace, and
/// CommonJS provenance is followed. Constructor arguments and subclass bodies
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("ui.notice")
        .description("Uses Obsidian notices")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::constructor_global("Notice"))
        .query(EventQuery::constructor_module("obsidian", "Notice"))
        .query(EventQuery::class_module("obsidian", "Notice"))
        .build()
        .unwrap()
}
