//! Browser global-input listener rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

const INPUT_EVENTS: [&str; 16] = [
    "keydown",
    "keyup",
    "paste",
    "copy",
    "cut",
    "mousedown",
    "mouseup",
    "mousemove",
    "pointerdown",
    "pointerup",
    "pointermove",
    "touchstart",
    "touchend",
    "dragstart",
    "drop",
    "input",
];

/// Detects rooted `document`, `window`, `self`, `globalThis`, and
/// `document.body` event-listener registrations for the listed keyboard,
/// clipboard, pointer, touch, drag/drop, and input events. The direct
/// `on*` property paths require rooted identity; property writes are retained
/// for invalidation but are not reported because the declarative vocabulary
/// has no rooted property-write occurrence.
pub fn rule() -> Rule {
    Rule::builder("browser.global-input-hook")
        .description("Registers global input handlers")
        .category(Category::new("browser/input").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_call_rooted("document.addEventListener")
                .with_arg_static_strings(0, INPUT_EVENTS))
        .query(QueryDecl::member_call_rooted("addEventListener")
                .with_arg_static_strings(0, INPUT_EVENTS))
        .query(QueryDecl::member_call_rooted("document.body.addEventListener")
                .with_arg_static_strings(0, INPUT_EVENTS))
        .query(QueryDecl::member_read_rooted("document.onkeydown"))
        .query(QueryDecl::member_read_rooted("document.onkeyup"))
        .query(QueryDecl::member_read_rooted("document.onkeypress"))
        .query(QueryDecl::member_read_rooted("document.onpaste"))
        .query(QueryDecl::member_read_rooted("document.oncopy"))
        .query(QueryDecl::member_read_rooted("document.oncut"))
        .build()
        .unwrap()
}
