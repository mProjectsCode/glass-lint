use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unbound `new Notice()` syntax plus `Notice` constructors and
/// subclasses proven to come from the `obsidian` module. The unbound form is
/// a syntactic heuristic; local/shadowed and reassigned names are excluded,
/// while ESM, namespace, and CommonJS module provenance is followed.
/// Constructor arguments and subclass bodies are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.notice")
        .label("Uses Obsidian notices")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_constructor("Notice"))
        .matcher(Matcher::module_constructor("obsidian", "Notice"))
        .matcher(Matcher::module_class("obsidian", "Notice"))
        .build()
        .unwrap()
}
