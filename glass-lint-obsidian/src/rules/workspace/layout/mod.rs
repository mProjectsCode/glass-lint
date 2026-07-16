//! Obsidian workspace-layout rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to `getLayout`, `changeLayout`, and
/// `requestSaveLayout` on `app.workspace`. Provenance follows `this.app`,
/// workspace aliases, static computed properties, source-ordered alias
/// reassignment, and lexical shadowing. Dynamic or unlisted members, local
/// lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.layout")
        .label("Reads or writes workspace layout")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_call("app.workspace.getLayout"))
        .matcher(Matcher::rooted_member_call("app.workspace.changeLayout"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.requestSaveLayout",
        ))
        .build()
        .unwrap()
}
