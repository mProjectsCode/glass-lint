use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects global/unbound `new Modal()` syntax and `Modal` constructors or
/// subclass expressions proven to originate from the `obsidian` module through
/// ESM, CommonJS, or namespace aliases. Local/shadowed and reassigned aliases
/// are excluded from the provenance matchers; the heuristic global spelling
/// remains syntactic, and constructor arguments and class bodies are ignored.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.modal")
        .label("Uses Obsidian modal UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_constructor("Modal"))
        .matcher(Matcher::module_constructor("obsidian", "Modal"))
        .matcher(Matcher::module_class("obsidian", "Modal"))
        .build()
        .unwrap()
}
