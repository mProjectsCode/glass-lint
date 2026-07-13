use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects `Modal` constructors and subclass expressions proven to originate
/// from the `obsidian` module through ESM, CommonJS, or namespace aliases.
/// Local, unbound, shadowed, and reassigned names are excluded; constructor
/// arguments and class bodies are ignored.
pub(crate) fn rule() -> Rule {
    Rule::builder("ui.modal")
        .label("Uses Obsidian modal UI")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_constructor("obsidian", "Modal"))
        .matcher(Matcher::module_class("obsidian", "Modal"))
        .build()
        .unwrap()
}
