//! Browser executable-script-injection rule definition.

use glass_lint_core::rules::{
    Category, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, MatcherDecl,
    ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule, Severity, ValueMatcher,
};

/// Detects rooted script elements whose executable content is configured and
/// then inserted into the document. Unused or disconnected elements fail
/// closed; direct document HTML sinks are checked separately.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.script-injection")
        .description("Injects executable script elements")
        .category(Category::new("browser/dom").unwrap())
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .declaration(MatcherDecl::from_object_flow(
            &ObjectFlowMatcher::builder("script-element")
                .source(
                    ObjectSourceMatcher::returned_by("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                )
                .configured_by(FlowCondition::any_of([
                    ObjectEventMatcher::property_write("src", ValueMatcher::static_string()),
                    ObjectEventMatcher::property_write("text", ValueMatcher::static_string()),
                    ObjectEventMatcher::property_write(
                        "textContent",
                        ValueMatcher::static_string(),
                    ),
                ]))
                .complete_at(FlowCompletion::any_sink([
                    FlowSinkMatcher::argument_of("document.head.appendChild", 0),
                    FlowSinkMatcher::argument_of("document.body.appendChild", 0),
                ]))
                .build()
                .unwrap(),
        ))
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.write")
                .arg(
                    0,
                    ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
                )
                .build()
                .unwrap(),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.writeln")
                .arg(
                    0,
                    ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
                )
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
