use glass_lint_core::rules::{
    EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
    LifecycleSink, QueryDecl, ValueMatcher,
};

use super::super::support::{self, Classification};

fn assert_count(result: &Classification, expected: usize) {
    assert_eq!(result.finding_count, expected);
}

fn lifecycle_with_source_and_sink(
    source: Result<EventQuery, glass_lint_core::rules::QueryBuildError>,
    sink: LifecycleSink,
) -> QueryDecl {
    QueryDecl::lifecycle(
        LifecycleQuery::catalog_builder("global lifecycle")
            .source(source)
            .condition(LifecycleCondition::event(LifecycleEvent::property_write(
                "ready",
                ValueMatcher::any_value(),
            )))
            .completion(LifecycleCompletion::any_sink([sink]))
            .build(),
    )
    .unwrap()
}

#[test]
fn lifecycle_tracks_objects_returned_by_global_calls() {
    let rule = support::rule("global-source").query(lifecycle_with_source_and_sink(
        EventQuery::call_global("fetch"),
        LifecycleSink::argument_of_member("document.head.appendChild", 0).unwrap(),
    ));
    let rule = rule.build().unwrap();
    assert_count(
        &support::classify(
            "const node = fetch(); node.ready = true; document.head.appendChild(node);",
            &[rule],
        ),
        1,
    );
}

#[test]
fn lifecycle_tracks_global_call_sinks() {
    let rule = support::rule("global-sink").query(lifecycle_with_source_and_sink(
        EventQuery::member_call_rooted("document.createElement"),
        LifecycleSink::argument_of_global("fetch", 0).unwrap(),
    ));
    let rule = rule.build().unwrap();
    assert_count(
        &support::classify(
            "const node = document.createElement('div'); node.ready = true; fetch(node);",
            &[rule],
        ),
        1,
    );
}
