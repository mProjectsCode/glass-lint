use glass_lint_core::rules::{
    Confidence, EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
    LifecycleQuery, QueryDecl, Rule, Severity, ValueMatcher,
};

/// Detects an input created by `document.createElement("input")` whose direct
/// `type` property is assigned the static value `"file"`. The bounded flow
/// follows direct aliases and respects reassignment. Static computed property
/// names and `setAttribute("type", "file")` are recognized; non-static type
/// values are not.
pub fn rule() -> Rule {
    Rule::catalog_builder("browser.file-dialog")
        .description("Uses browser file input dialogs")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::lifecycle(
            LifecycleQuery::catalog_builder("file input element")
                .source(
                    EventQuery::member_call_rooted("document.createElement")
                        .unwrap()
                        .with_arg(
                            0,
                            ValueMatcher::static_string().try_equals("input").unwrap(),
                        ),
                )
                .condition(LifecycleCondition::any_of([
                    LifecycleEvent::property_write(
                        "type",
                        ValueMatcher::static_string().try_equals("file").unwrap(),
                    ),
                    Ok(LifecycleEvent::member_call("setAttribute")
                        .unwrap()
                        .arg(0, ValueMatcher::static_string().try_equals("type").unwrap())
                        .unwrap()
                        .arg(1, ValueMatcher::static_string().try_equals("file").unwrap())
                        .unwrap()
                        .build()),
                ]))
                .completion(LifecycleCompletion::configuration())
                .build(),
        ))
        .query(EventQuery::member_call_rooted("showOpenFilePicker"))
        .query(EventQuery::member_call_rooted("showSaveFilePicker"))
        .build()
        .unwrap()
}
