//! Obsidian workspace-layout rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted calls to `getLayout`, `changeLayout`, and
/// `requestSaveLayout` on `app.workspace`. Provenance follows `this.app`,
/// workspace aliases, static computed properties, source-ordered alias
/// reassignment, and lexical shadowing. Dynamic or unlisted members, local
/// lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.layout")
        .description("Reads or writes workspace layout")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getLayout")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.changeLayout")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.requestSaveLayout")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
