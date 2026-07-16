//! Obsidian notice rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the exact global `Notice` constructor plus constructors and
/// subclasses proven to come from the `obsidian` module. Local/shadowed and
/// reassigned names are excluded, while global-object, ESM, namespace, and
/// CommonJS provenance is followed. Constructor arguments and subclass bodies
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("ui.notice")
        .label("Uses Obsidian notices")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::global_constructor("Notice"))
        .matcher(Matcher::module_constructor("obsidian", "Notice"))
        .matcher(Matcher::module_class("obsidian", "Notice"))
        .build()
        .unwrap()
}
