//! Browser global-input listener rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .category("browser/input")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS)
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("addEventListener")
                .arg_static_strings(0, INPUT_EVENTS)
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.body.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS)
                .build()
                .unwrap(),
        )
        .declaration(MatcherDecl::rooted_member_read("document.onkeydown"))
        .declaration(MatcherDecl::rooted_member_read("document.onkeyup"))
        .declaration(MatcherDecl::rooted_member_read("document.onkeypress"))
        .declaration(MatcherDecl::rooted_member_read("document.onpaste"))
        .declaration(MatcherDecl::rooted_member_read("document.oncopy"))
        .declaration(MatcherDecl::rooted_member_read("document.oncut"))
        .build()
        .unwrap()
}
