//! Obsidian workspace-event registration rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity, ValueMatcher};

/// Detects rooted `app.workspace.on` registrations for the documented
/// workspace and editor/menu events. Rooted aliases, static computed names,
/// source-ordered reassignment, and lexical shadowing are handled by the
/// matcher; dynamic event names and unrelated emitters are excluded.
pub fn rule() -> Rule {
    Rule::builder("workspace.events")
        .description("Registers Obsidian workspace events")
        .category(Category::new("workspace").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(
            EventQuery::member_call_rooted("app.workspace.on")
                .map(|q| {
                    q.with_arg(
                        0,
                        ValueMatcher::static_string().equals_any([
                            "active-leaf-change",
                            "file-open",
                            "layout-change",
                            "window-open",
                            "window-close",
                            "quit",
                            "editor-change",
                            "editor-paste",
                            "editor-drop",
                            "file-menu",
                            "editor-menu",
                        ]),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
