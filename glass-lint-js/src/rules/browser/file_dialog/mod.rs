use glass_lint_core::rules::{Category, 
    Confidence, FlowCompletion, FlowCondition, MatcherDecl, ObjectEventMatcher, ObjectFlowMatcher,
    ObjectSourceMatcher, Rule, Severity, ValueMatcher,
};

/// Detects an input created by `document.createElement("input")` whose direct
/// `type` property is assigned the static value `"file"`. The bounded flow
/// follows direct aliases and respects reassignment. Static computed property
/// names are normalized; `setAttribute` and non-static type values are not
/// recognized as configuration evidence.
pub fn rule() -> Rule {
    Rule::builder("browser.file-dialog")
        .description("Uses browser file input dialogs")
        .category(Category::new("browser/file-dialog").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::from_object_flow(
            &ObjectFlowMatcher::builder("file input element")
                .source(
                    ObjectSourceMatcher::returned_by("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("input")),
                )
                .configured_by(FlowCondition::any_of([
                    ObjectEventMatcher::property_write(
                        "type",
                        ValueMatcher::static_string().equals("file"),
                    ),
                    ObjectEventMatcher::member_call("setAttribute")
                        .arg(0, ValueMatcher::static_string().equals("type"))
                        .arg(1, ValueMatcher::static_string().equals("file"))
                        .build(),
                ]))
                .complete_at(FlowCompletion::configuration())
                .build()
                .unwrap(),
        ))
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("showOpenFilePicker")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("showSaveFilePicker")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
