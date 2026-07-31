//! Browser executable-script-injection rule definition.

use glass_lint_core::rules::{
    Category, Confidence, EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
    LifecycleQuery, LifecycleSink, QueryDecl, Rule, Severity, ValueMatcher,
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
        .query(QueryDecl::lifecycle(
            LifecycleQuery::builder("script-element")
                .source(
                    EventQuery::member_call_rooted("document.createElement")
                        .unwrap()
                        .with_arg(0, ValueMatcher::static_string().equals("script")),
                )
                .condition(LifecycleCondition::any_of([
                    LifecycleEvent::property_write("src", ValueMatcher::static_string()),
                    LifecycleEvent::property_write("text", ValueMatcher::static_string()),
                    LifecycleEvent::property_write("textContent", ValueMatcher::static_string()),
                    Ok(LifecycleEvent::member_call("setAttribute")
                        .unwrap()
                        .arg(0, ValueMatcher::static_string().equals("src"))
                        .unwrap()
                        .arg(1, ValueMatcher::static_string())
                        .unwrap()
                        .build()),
                ]))
                .completion(LifecycleCompletion::any_sink([
                    LifecycleSink::argument_of("document.head.appendChild", 0),
                    LifecycleSink::argument_of("document.body.appendChild", 0),
                    LifecycleSink::argument_of("document.documentElement.appendChild", 0),
                    LifecycleSink::argument_of("document.documentElement.insertBefore", 0),
                ]))
                .build(),
        ))
        .query(
            EventQuery::member_call_rooted("document.write")
                .map(|q| {
                    q.with_arg(
                        0,
                        ValueMatcher::static_string()
                            .contains_any(["<script", "javascript:"])
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("document.writeln")
                .map(|q| {
                    q.with_arg(
                        0,
                        ValueMatcher::static_string()
                            .contains_any(["<script", "javascript:"])
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("document.body.insertAdjacentHTML")
                .map(|q| {
                    q.with_arg(
                        1,
                        ValueMatcher::static_string()
                            .contains_any(["<script", "javascript:"])
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .query(
            EventQuery::member_call_rooted("document.documentElement.insertAdjacentHTML")
                .map(|q| {
                    q.with_arg(
                        1,
                        ValueMatcher::static_string()
                            .contains_any(["<script", "javascript:"])
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
