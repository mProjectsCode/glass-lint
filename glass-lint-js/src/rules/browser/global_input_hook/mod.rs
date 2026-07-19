//! Browser global-input listener rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

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
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("addEventListener").arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.body.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::rooted_member_read("document.onkeydown"))
        .matcher(Matcher::rooted_member_read("document.onkeyup"))
        .matcher(Matcher::rooted_member_read("document.onkeypress"))
        .matcher(Matcher::rooted_member_read("document.onpaste"))
        .matcher(Matcher::rooted_member_read("document.oncopy"))
        .matcher(Matcher::rooted_member_read("document.oncut"))
        .build()
        .unwrap()
}
