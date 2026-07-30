use glass_lint_core::rules::{
    Category, Confidence, EventQuery, EventRequirement, LifecycleCompletion, LifecycleCondition,
    LifecycleEvent, LifecycleQuery, LifecycleSink, LifecycleSource, QueryDecl, Rule, Severity,
    ValueMatcher,
};

fn rule<Q: glass_lint_core::rules::IntoQueryDecl>(id: &str, description: &str, query: Q) -> Rule {
    Rule::builder(id)
        .description(description)
        .category(Category::new("example").expect("static category"))
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(query)
        .build()
        .expect("static example rule")
}

fn ordinary() -> Rule {
    rule(
        "network.request",
        "Calls the network API",
        EventQuery::call_global("fetch").unwrap().into_query(),
    )
}

fn constrained() -> Rule {
    let event = EventQuery::call_global("fetch")
        .expect("valid identity")
        .with_arg_static_strings(0, ["https://", "http://"])
        .expect("valid argument constraint");
    rule("network.remote", "Calls a remote URL", event.into_query())
}

fn alternatives() -> Rule {
    let query = QueryDecl::any([
        EventQuery::call_global("fetch").map(EventQuery::into_query),
        EventQuery::call_global("XMLHttpRequest").map(EventQuery::into_query),
    ])
    .expect("compatible alternatives");
    rule("network.alternative", "Uses a network API", query)
}

fn same_event_conjunction() -> Rule {
    let query = QueryDecl::all(
        EventQuery::call_global("fetch"),
        [EventRequirement::argument(
            0,
            ValueMatcher::static_string()
                .starts_with_any(["https://"])
                .expect("value set"),
        )],
    )
    .expect("same-event requirements");
    rule("network.constrained", "Calls fetch with a URL", query)
}

fn returned_and_instance() -> (Rule, Rule) {
    (
        rule(
            "dom.returned",
            "Uses an object returned by a rooted call",
            QueryDecl::member_call_returned("document.createElement", "setAttribute")
                .expect("valid returned-object relation"),
        ),
        rule(
            "dom.instance",
            "Uses an instance created by a module export",
            QueryDecl::member_call_instance("pkg", "Client", "send")
                .expect("valid instance relation"),
        ),
    )
}

fn lifecycle() -> Rule {
    let lifecycle = LifecycleQuery::builder("remote element")
        .source(Ok(LifecycleSource::returned_by("document.createElement")
            .expect("valid source")
            .arg(0, ValueMatcher::static_string().equals("script"))
            .expect("valid source argument")))
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::static_string()
                .starts_with_any(["https://"])
                .expect("valid URL alternatives"),
        )))
        .completion(LifecycleCompletion::any_sink([LifecycleSink::argument_of(
            "document.body.appendChild",
            0,
        )]))
        .build()
        .expect("valid lifecycle");
    rule(
        "dom.remote",
        "Loads a remote script element",
        QueryDecl::lifecycle(Ok(lifecycle)),
    )
}

fn correlated_sinks() -> Rule {
    let lifecycle = LifecycleQuery::builder("two sinks")
        .source(Ok(
            LifecycleSource::returned_by("document.createElement").expect("valid source")
        ))
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::all_sinks([
            LifecycleSink::argument_of("document.head.appendChild", 0).expect("valid sink"),
            LifecycleSink::argument_of("document.body.appendChild", 0).expect("valid sink"),
        ]))
        .build()
        .expect("valid lifecycle");
    rule(
        "dom.two-sinks",
        "Uses one object at two later sinks",
        QueryDecl::lifecycle(Ok(lifecycle)),
    )
}

fn structured_error() -> glass_lint_core::rules::QueryBuildError {
    EventQuery::call_global("").expect_err("empty identities are rejected")
}

fn main() {
    let _rules = [
        ordinary(),
        correlated_sinks(),
        constrained(),
        alternatives(),
        same_event_conjunction(),
        returned_and_instance().0,
        returned_and_instance().1,
        lifecycle(),
    ];
    assert!(structured_error().to_string().contains("identity"));
}
