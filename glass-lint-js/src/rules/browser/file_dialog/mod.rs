use glass_lint_core::rules::{
    Confidence, FlowCompletion, FlowCondition, MemberCallMatcher, ObjectEventMatcher,
    ObjectFlowMatcher, ObjectSourceMatcher, Rule, Severity, ValueMatcher,
};

/// Detects an input created by `document.createElement("input")` whose direct
/// `type` property is assigned the static value `"file"`. The bounded flow
/// follows direct aliases and respects reassignment. Static computed property
/// names are normalized; `setAttribute` and non-static type values are not
/// recognized as configuration evidence.
pub fn rule() -> Rule {
    Rule::builder("browser.file-dialog")
        .label("Uses browser file input dialogs")
        .category("browser/file-dialog")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(
            ObjectFlowMatcher::builder("file input element")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("input")),
                ))
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::static_string().equals("file"),
                )))
                .complete_at(FlowCompletion::configuration())
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
