//! Browser executable-script-injection rule definition.

use glass_lint_core::rules::{
    Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher, MemberCallMatcher,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule, Severity, ValueMatcher,
};

/// Detects rooted script elements whose executable content is configured and
/// then inserted into the document. Unused or disconnected elements fail
/// closed; direct document HTML sinks are checked separately.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.script-injection")
        .description("Injects executable script elements")
        .category("browser/dom")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::from(
            ObjectFlowMatcher::builder("script-element")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg_static_strings(0, ["script"]),
                ))
                .configured_by(FlowCondition::any_of([
                    ObjectEventMatcher::property_write("src", ValueMatcher::static_string()),
                    ObjectEventMatcher::property_write("text", ValueMatcher::static_string()),
                    ObjectEventMatcher::property_write(
                        "textContent",
                        ValueMatcher::static_string(),
                    ),
                ]))
                .complete_at(FlowCompletion::any_sink([
                    FlowSinkMatcher::argument_of(
                        MemberCallMatcher::rooted("document.head.appendChild"),
                        0,
                    ),
                    FlowSinkMatcher::argument_of(
                        MemberCallMatcher::rooted("document.body.appendChild"),
                        0,
                    ),
                ]))
                .build(),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.write").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.writeln").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .build()
        .unwrap()
}
