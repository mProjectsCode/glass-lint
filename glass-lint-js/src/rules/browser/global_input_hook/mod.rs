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
/// `on*` property paths remain heuristic; event names must resolve to one of
/// the configured static strings.
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
            MemberCallMatcher::rooted("window.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("globalThis.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.body.addEventListener")
                .arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("self.addEventListener").arg_static_strings(0, INPUT_EVENTS),
        ))
        .matcher(Matcher::heuristic_member_read("document.onkeydown"))
        .matcher(Matcher::heuristic_member_read("document.onkeyup"))
        .matcher(Matcher::heuristic_member_read("document.onkeypress"))
        .matcher(Matcher::heuristic_member_read("document.onpaste"))
        .matcher(Matcher::heuristic_member_read("document.oncopy"))
        .matcher(Matcher::heuristic_member_read("document.oncut"))
        .matcher(Matcher::heuristic_member_read("window.onkeydown"))
        .matcher(Matcher::heuristic_member_read("window.onkeyup"))
        .matcher(Matcher::heuristic_member_read("window.onkeypress"))
        .matcher(Matcher::heuristic_member_read("window.onpaste"))
        .matcher(Matcher::heuristic_member_read("window.oncopy"))
        .matcher(Matcher::heuristic_member_read("window.oncut"))
        .build()
        .unwrap()
}
