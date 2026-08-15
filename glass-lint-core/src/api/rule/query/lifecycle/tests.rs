use super::*;
use crate::api::rule::ValueMatcher;

fn source() -> Result<EventQuery, QueryBuildError> {
    EventQuery::member_call_rooted("document.createElement")
}

#[test]
fn explicit_completion_and_conditions_build() {
    let lc = LifecycleQuery::catalog_builder("input")
        .source(source())
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "type",
            ValueMatcher::static_string().try_equals("file").unwrap(),
        )))
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    assert_eq!(lc.sources.len(), 1);
    assert!(lc.condition.is_some());
    assert!(lc.completion.is_some());
}

#[test]
fn deferred_builder_reports_first_invalid_operation() {
    let condition = || {
        LifecycleCondition::event(LifecycleEvent::property_write(
            "value",
            ValueMatcher::static_string().try_equals("value").unwrap(),
        ))
    };
    let completion = || LifecycleCompletion::configuration();
    let error = LifecycleQuery::catalog_builder("input")
        .source(source())
        .condition(condition())
        .condition(condition())
        .completion(completion())
        .completion(completion())
        .build()
        .expect_err("duplicate condition should be retained");
    assert_eq!(error, QueryBuildError::DuplicateLifecycleStage("condition"));
}

#[test]
fn immediate_builder_reports_duplicate_stages_at_build() {
    let condition = LifecycleCondition::event(LifecycleEvent::property_write(
        "value",
        ValueMatcher::any_value(),
    ))
    .unwrap();
    let error = LifecycleQuery::builder("input")
        .try_source(source())
        .unwrap()
        .condition(condition.clone())
        .condition(condition)
        .completion(LifecycleCompletion::configuration())
        .build()
        .expect_err("duplicate condition should be retained");
    assert_eq!(error, QueryBuildError::DuplicateLifecycleStage("condition"));
}

#[test]
fn deferred_condition_accepts_a_prebuilt_value() {
    let condition = LifecycleCondition::event(LifecycleEvent::property_write(
        "type",
        ValueMatcher::any_value(),
    ))
    .unwrap();
    let lifecycle = LifecycleQuery::catalog_builder("input")
        .source(source())
        .condition(condition)
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    assert!(lifecycle.condition.is_some());
}

#[test]
fn empty_sources_fail() {
    let err = LifecycleQuery::catalog_builder("empty")
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("source"));
}

#[test]
fn lifecycle_source_accepts_event_query() {
    let query = EventQuery::call_global("fetch").unwrap();
    let lifecycle = LifecycleQuery::catalog_builder("fetch result")
        .source(query)
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "url",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    assert_eq!(
        lifecycle.sources(),
        &[EventQuery::call_global("fetch").unwrap()]
    );
}

#[test]
fn order_independent_lifecycle_alternatives_are_canonical() {
    let src = LifecycleEvent::property_write("src", ValueMatcher::any_value()).unwrap();
    let href = LifecycleEvent::property_write("href", ValueMatcher::any_value()).unwrap();
    assert_eq!(
        LifecycleCondition::any_of([src.clone(), href.clone()]).unwrap(),
        LifecycleCondition::any_of([href, src]).unwrap()
    );

    let first = LifecycleSink::argument_of_member("sink", 0).unwrap();
    let second = LifecycleSink::any_argument_of_member("other").unwrap();
    assert_eq!(
        LifecycleCompletion::any_sink([first.clone(), second.clone()]).unwrap(),
        LifecycleCompletion::any_sink([second, first]).unwrap()
    );
}

#[test]
fn all_of_conditions_are_canonical() {
    let first = LifecycleEvent::property_write("first", ValueMatcher::any_value()).unwrap();
    let second = LifecycleEvent::property_write("second", ValueMatcher::any_value()).unwrap();
    let a = LifecycleCondition::all_of([first.clone(), second.clone(), first.clone()]).unwrap();
    let b = LifecycleCondition::all_of([second, first]).unwrap();
    assert_eq!(a, b);
    assert!(matches!(a.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 2));
}

#[test]
fn lifecycle_collections_enforce_their_bounds_at_construction() {
    let events = (0..=limits::MAX_LIFECYCLE_EVENTS)
        .map(|index| {
            LifecycleEvent::property_write(format!("property-{index}"), ValueMatcher::any_value())
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(matches!(
        LifecycleCondition::all_of(events),
        Err(QueryBuildError::CollectionTooLarge(
            "lifecycle condition events",
            _
        ))
    ));

    let sinks = (0..=limits::MAX_LIFECYCLE_SINKS)
        .map(|index| LifecycleSink::argument_of_member(format!("sink-{index}"), 0))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(matches!(
        LifecycleCompletion::all_sinks(sinks),
        Err(QueryBuildError::CollectionTooLarge(
            "lifecycle completion sinks",
            _
        ))
    ));
}

#[test]
fn all_sink_completion_is_bounded_and_deterministic() {
    let first = LifecycleSink::argument_of_member("document.head.appendChild", 0).unwrap();
    let second = LifecycleSink::argument_of_member("document.body.appendChild", 0).unwrap();
    let a = LifecycleCompletion::all_sinks([first.clone(), second.clone(), first.clone()]).unwrap();
    let b = LifecycleCompletion::all_sinks([second, first]).unwrap();
    assert_eq!(a, b);
    assert!(matches!(a.kind(), LifecycleCompletionKind::AllSinks(sinks) if sinks.len() == 2));
}

#[test]
fn lifecycle_source_arg_adds_argument_constraint() {
    let s = EventQuery::member_call_rooted("foo.bar")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("val").unwrap());
    let s = s.unwrap();
    assert_eq!(s.constraints().len(), 1);
    assert_eq!(s.constraints()[0].arg_index().get(), 0);
}

#[test]
fn lifecycle_argument_limits_count_groups_and_per_group_predicates() {
    let mut source = EventQuery::member_call_rooted("foo.bar").unwrap();
    for index in 0..limits::MAX_ARGUMENT_GROUPS {
        source = source.with_arg(index, ValueMatcher::any_value()).unwrap();
    }
    assert_eq!(source.constraints().len(), limits::MAX_ARGUMENT_GROUPS);
    assert!(matches!(
        source.with_arg(limits::MAX_ARGUMENT_GROUPS, ValueMatcher::any_value()),
        Err(QueryBuildError::ExcessiveArgumentGroups(_))
    ));

    let mut event = LifecycleEvent::member_call("foo").unwrap();
    for _ in 0..limits::MAX_PREDICATES_PER_ARGUMENT {
        event = event.arg(0, ValueMatcher::any_value()).unwrap();
    }
    assert!(matches!(
        event.arg(0, ValueMatcher::any_value()),
        Err(QueryBuildError::ExcessivePredicates { index: 0, .. })
    ));

    let query = crate::api::rule::EventQuery::call_global("foo").unwrap();
    let mut query = query;
    for index in 0..limits::MAX_ARGUMENT_GROUPS {
        query = query.with_arg(index, ValueMatcher::any_value()).unwrap();
    }
    assert_eq!(query.constraints().len(), limits::MAX_ARGUMENT_GROUPS);
}

#[test]
fn lifecycle_event_property_write_holds_property_and_value() {
    let value = ValueMatcher::any_value();
    let event = LifecycleEvent::property_write("src", value).unwrap();
    assert!(
        matches!(event.kind(), LifecycleEventKind::PropertyWrite { property, .. } if property == "src")
    );
}

#[test]
fn lifecycle_event_property_write_trims_and_rejects_whitespace_only_names() {
    let event = LifecycleEvent::property_write(" src ", ValueMatcher::any_value()).unwrap();
    assert!(
        matches!(event.kind(), LifecycleEventKind::PropertyWrite { property, .. } if property == "src")
    );
    assert!(matches!(
        LifecycleEvent::property_write("  ", ValueMatcher::any_value()),
        Err(QueryBuildError::EmptyIdentityName)
    ));
}

#[test]
fn lifecycle_event_member_call_builds_with_args() {
    let event: LifecycleEvent = LifecycleEvent::member_call("addEventListener")
        .unwrap()
        .arg(0, ValueMatcher::static_string().try_equals("load").unwrap())
        .unwrap()
        .build();
    assert!(
        matches!(event.kind(), LifecycleEventKind::MemberCall { member, .. } if member.as_str() == "addEventListener")
    );
}

#[test]
fn lifecycle_event_text_and_argument_indices_are_checked() {
    assert!(matches!(
        LifecycleEvent::property_write("", ValueMatcher::any_value()),
        Err(QueryBuildError::EmptyIdentityName)
    ));
    assert!(matches!(
        LifecycleEvent::member_call(""),
        Err(QueryBuildError::EmptyIdentityName)
    ));
    assert!(matches!(
        LifecycleEvent::member_call("setAttribute")
            .unwrap()
            .arg(256, ValueMatcher::any_value()),
        Err(QueryBuildError::InvalidArgumentIndex(256))
    ));
}

#[test]
fn lifecycle_condition_any_of_accepts_multiple_events() {
    let condition = LifecycleCondition::any_of([
        LifecycleEvent::property_write("a", ValueMatcher::any_value()),
        LifecycleEvent::property_write("b", ValueMatcher::any_value()),
    ])
    .unwrap();
    assert!(matches!(condition.kind(), LifecycleConditionKind::AnyOf(events) if events.len() == 2));
}

#[test]
fn lifecycle_condition_all_of_accepts_multiple_events() {
    let condition = LifecycleCondition::all_of([LifecycleEvent::property_write(
        "x",
        ValueMatcher::any_value(),
    )])
    .unwrap();
    assert!(matches!(condition.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 1));
}

#[test]
fn lifecycle_condition_event_wraps_in_all_of() {
    let condition = LifecycleCondition::event(LifecycleEvent::property_write(
        "type",
        ValueMatcher::static_string().try_equals("file").unwrap(),
    ))
    .unwrap();
    assert!(matches!(condition.kind(), LifecycleConditionKind::AllOf(events) if events.len() == 1));
}

#[test]
fn lifecycle_completion_configuration_has_no_sinks() {
    let completion = LifecycleCompletion::configuration();
    assert!(matches!(
        completion.kind(),
        LifecycleCompletionKind::Configuration
    ));
}

#[test]
fn lifecycle_completion_any_sink_holds_sink_matchers() {
    let sink = LifecycleSink::argument_of_member("target.appendChild", 0).unwrap();
    let completion = LifecycleCompletion::any_sink([sink]).unwrap();
    assert!(
        matches!(completion.kind(), LifecycleCompletionKind::AnySink(sinks) if sinks.len() == 1)
    );
}

#[test]
fn lifecycle_sink_argument_of_holds_chain_and_index() {
    let sink = LifecycleSink::argument_of_member("parent.appendChild", 0).unwrap();
    assert_eq!(sink.chain(), "parent.appendChild");
    assert!(matches!(
        sink.kind(),
        LifecycleSinkKind::ArgumentOf { index, .. } if index.get() == 0
    ));
}

#[test]
fn lifecycle_sink_any_argument_of_holds_chain() {
    let sink = LifecycleSink::any_argument_of_member("parent.appendChild").unwrap();
    assert_eq!(sink.chain(), "parent.appendChild");
    assert!(matches!(
        sink.kind(),
        LifecycleSinkKind::AnyArgumentOf { .. }
    ));
}

#[test]
fn configuration_completion_requires_condition() {
    let err = LifecycleQuery::catalog_builder("test")
        .source(source())
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(
        err.to_string().contains("condition"),
        "configuration completion without condition: {err}"
    );
}

#[test]
fn any_sink_requires_non_empty_sinks() {
    let err = LifecycleQuery::catalog_builder("test")
        .source(source())
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "x",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink(Vec::<
            Result<LifecycleSink, QueryBuildError>,
        >::new()))
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("sink"), "empty any_sink: {err}");
}

#[test]
fn completion_is_required() {
    let err = LifecycleQuery::catalog_builder("test")
        .source(source())
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "x",
            ValueMatcher::any_value(),
        )))
        .build()
        .unwrap_err();
    assert!(
        err.to_string().contains("completion"),
        "missing completion: {err}"
    );
}

#[test]
fn empty_any_of_condition_fails() {
    let condition = LifecycleCondition::any_of::<[LifecycleEvent; 0]>([]);
    let err = LifecycleQuery::catalog_builder("test")
        .source(source())
        .condition(condition)
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("target.appendChild", 0),
        ]))
        .build()
        .unwrap_err();
    assert!(
        err.to_string().contains("condition"),
        "empty any_of condition: {err}"
    );
}

#[test]
fn empty_all_of_condition_fails() {
    let condition = LifecycleCondition::all_of::<[LifecycleEvent; 0]>([]);
    let err = LifecycleQuery::catalog_builder("test")
        .source(source())
        .condition(condition)
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("target.appendChild", 0),
        ]))
        .build()
        .unwrap_err();
    assert!(
        err.to_string().contains("condition"),
        "empty all_of condition: {err}"
    );
}

#[test]
fn try_source_reports_constructor_errors_at_the_call_site() {
    let error = LifecycleQuery::builder("test")
        .try_source(EventQuery::member_call_rooted(""))
        .unwrap_err();
    assert!(matches!(error, QueryBuildError::MalformedChain(_)));
}
