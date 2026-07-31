//! Browser global-input listener rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

const INPUT_EVENTS: [&str; 27] = [
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
    "beforeinput",
    "compositionstart",
    "compositionupdate",
    "compositionend",
    "contextmenu",
    "wheel",
    "pointercancel",
    "touchmove",
    "touchcancel",
    "dragover",
    "dragend",
];

/// Detects rooted `document`, `window`, `self`, `globalThis`, and
/// `document.body` event-listener registrations for the listed keyboard,
/// clipboard, pointer, touch, drag/drop, composition, and input events. It
/// also reports writes to rooted `on*` handler properties, while reads remain
/// outside the rule.
pub fn rule() -> Rule {
    Rule::builder("browser.global-input-hook")
        .description("Registers global input handlers")
        .category(Category::new("browser/input").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(
            EventQuery::member_call_rooted("document.addEventListener")
                .map(|q| {
                    q.with_arg_static_strings(0, INPUT_EVENTS)
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("addEventListener")
                .map(|q| {
                    q.with_arg_static_strings(0, INPUT_EVENTS)
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("document.body.addEventListener")
                .map(|q| {
                    q.with_arg_static_strings(0, INPUT_EVENTS)
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .query(EventQuery::property_write_rooted("document.onkeydown"))
        .query(EventQuery::property_write_rooted("document.onkeyup"))
        .query(EventQuery::property_write_rooted("document.onkeypress"))
        .query(EventQuery::property_write_rooted("document.onpaste"))
        .query(EventQuery::property_write_rooted("document.oncopy"))
        .query(EventQuery::property_write_rooted("document.oncut"))
        .query(EventQuery::property_write_rooted("window.onkeydown"))
        .query(EventQuery::property_write_rooted("window.onkeyup"))
        .query(EventQuery::property_write_rooted("window.onpaste"))
        .query(EventQuery::property_write_rooted("window.oncopy"))
        .query(EventQuery::property_write_rooted("window.oncut"))
        .query(EventQuery::property_write_rooted("self.onkeydown"))
        .query(EventQuery::property_write_rooted("self.onkeyup"))
        .query(EventQuery::property_write_rooted("self.onpaste"))
        .query(EventQuery::property_write_rooted("globalThis.onkeydown"))
        .query(EventQuery::property_write_rooted("globalThis.onkeyup"))
        .query(EventQuery::property_write_rooted("document.body.onkeydown"))
        .query(EventQuery::property_write_rooted("document.body.onkeyup"))
        .build()
        .unwrap()
}
