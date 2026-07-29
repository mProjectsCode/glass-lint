use glass_lint_core::rules::{
    Category, Confidence, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
    LifecycleSource, QueryDecl, Rule, Severity, ValueMatcher,
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
        .query(QueryDecl::lifecycle(
            LifecycleQuery::builder("file input element")
                .source(
                    LifecycleSource::returned_by("document.createElement")
                        .unwrap()
                        .arg(0, ValueMatcher::static_string().equals("input")),
                )
                .condition(LifecycleCondition::any_of([
                    LifecycleEvent::property_write(
                        "type",
                        ValueMatcher::static_string().equals("file"),
                    ),
                    LifecycleEvent::member_call("setAttribute")
                        .unwrap()
                        .arg(0, ValueMatcher::static_string().equals("type"))
                        .unwrap()
                        .arg(1, ValueMatcher::static_string().equals("file"))
                        .unwrap()
                        .build(),
                ]))
                .completion(LifecycleCompletion::configuration())
                .build(),
        ))
        .query(QueryDecl::member_call_rooted("showOpenFilePicker"))
        .query(QueryDecl::member_call_rooted("showSaveFilePicker"))
        .build()
        .unwrap()
}
