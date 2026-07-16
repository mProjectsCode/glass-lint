use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects the syntactic lifecycle-registration chains
/// `this.registerEvent`, `this.registerDomEvent`, and `this.registerInterval`.
/// This medium-confidence heuristic does not prove an Obsidian plugin
/// receiver and does not follow aliases or reassignment. Static computed names
/// are accepted; other receivers, dynamic properties, and near-name methods
/// are excluded.
pub fn rule() -> Rule {
    Rule::builder("lifecycle.events")
        .label("Registers Obsidian lifecycle events")
        .category("lifecycle")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerEvent",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerDomEvent",
        ))
        .matcher(Matcher::instance_member_call(
            "obsidian",
            "Plugin",
            "registerInterval",
        ))
        .build()
        .unwrap()
}
