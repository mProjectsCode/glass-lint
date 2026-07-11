use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic `this.saveData()` plugin-storage call, including a
/// statically computed `saveData` property. This medium-confidence heuristic
/// does not prove an Obsidian plugin receiver and does not follow aliases,
/// shadowing, or reassignment; exact other receivers, dynamic properties, and
/// near-name methods are excluded, and arguments are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("storage.plugin-data-write")
        .label("Writes plugin data")
        .category("storage")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian", "Plugin", "saveData",
        ))
        .build()
        .unwrap()
}
