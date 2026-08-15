use crate::api::rule::query::{
    ArgumentMatcher, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryExpr, QueryExprKind,
    QueryPredicate, ValueMatcher, lifecycle, value, value::ArgumentConstraint,
};

pub(crate) fn explain_expression(expression: &QueryExpr) -> String {
    match expression.kind() {
        QueryExprKind::Event(query) => explain_event(query),
        QueryExprKind::SelectEvent(selection) => format!("event {} is selected", selection.bind),
        QueryExprKind::Require(predicate) => explain_predicate(predicate),
        QueryExprKind::Any(any) => format!(
            "any of: {}",
            any.iter()
                .map(explain_expression)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        QueryExprKind::All(all) => format!(
            "all of: {}",
            all.iter()
                .map(explain_expression)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        QueryExprKind::Lifecycle(lifecycle) => explain_lifecycle(lifecycle),
    }
}

fn explain_event(query: &EventQuery) -> String {
    let event = match &query.event {
        EventSpec::Call => format!("a call to {}", explain_identity(&query.identity)),
        EventSpec::Construct => format!(
            "a constructor call to {}",
            explain_identity(&query.identity)
        ),
        EventSpec::MemberCall { member } => format!(
            "a member call to `{member}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::MemberRead { member } => format!(
            "a member read of `{member}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::PropertyWrite { property } => format!(
            "a property write to `{property}` on {}",
            explain_identity(&query.identity)
        ),
        EventSpec::ClassReference => {
            format!("a class reference to {}", explain_identity(&query.identity))
        }
        EventSpec::Import => format!("an import of {}", explain_identity(&query.identity)),
        EventSpec::StringReference => {
            format!(
                "a string reference containing `{}`",
                query.identity.display_name()
            )
        }
    };
    append_constraints(event, &query.constraints)
}

fn explain_identity(identity: &IdentitySpec) -> String {
    match identity {
        IdentitySpec::Global { name } => format!("the global `{name}`"),
        IdentitySpec::Heuristic { name } => format!("the heuristic name `{name}`"),
        IdentitySpec::ModuleExport { module, export } => {
            format!("the `{export}` export from module `{module}`")
        }
        IdentitySpec::PackageModuleExport { module, export } => {
            format!("the `{export}` export from package/module `{module}`")
        }
        IdentitySpec::ModuleNamespace { module } => {
            format!("the namespace imported from module `{module}`")
        }
        IdentitySpec::PackageModuleNamespace { module } => {
            format!("the namespace imported from package/module `{module}`")
        }
        IdentitySpec::Rooted { path } => format!("the rooted path `{path}`"),
        IdentitySpec::LiteralString { predicate } => format!("a string matching `{predicate}`"),
        IdentitySpec::PackageSpecifier { pattern } => {
            format!("the package specifier `{pattern}`")
        }
        IdentitySpec::PrivateNetworkAddress => "a private or special-use network address".into(),
    }
}

fn append_constraints(mut description: String, constraints: &[ArgumentConstraint]) -> String {
    if !constraints.is_empty() {
        let rendered = constraints
            .iter()
            .map(|constraint| {
                format!(
                    "argument {} matches {}",
                    constraint.arg_index().get(),
                    explain_argument_matcher(constraint.predicate())
                )
            })
            .collect::<Vec<_>>()
            .join(" and ");
        description.push_str(" with ");
        description.push_str(&rendered);
    }
    description
}

fn explain_predicate(predicate: &QueryPredicate) -> String {
    match predicate {
        QueryPredicate::EventKind { event, expected } => match expected {
            EventSpec::MemberCall { member } => {
                format!("event {event} is a member call to `{member}`")
            }
            EventSpec::MemberRead { member } => {
                format!("event {event} is a member read of `{member}`")
            }
            expected => format!("event {event} is a {}", expected.diagnostic_name()),
        },
        QueryPredicate::EventIdentity { event, expected } => {
            format!("event {event} has identity {}", explain_identity(expected))
        }
        QueryPredicate::Argument {
            call,
            index,
            matcher,
        } => format!(
            "argument {}[{}] matches {}",
            call,
            index.get(),
            explain_argument_matcher(matcher)
        ),
        QueryPredicate::ReturnedObject { bind, identity } => {
            format!(
                "object {bind} is returned by {}",
                explain_identity(identity)
            )
        }
        QueryPredicate::ConstructedObject { bind, identity } => {
            format!(
                "object {bind} is constructed by {}",
                explain_identity(identity)
            )
        }
        QueryPredicate::MemberSubject { event, object } => {
            format!("event {event} uses object {object} as its member receiver")
        }
    }
}

fn explain_argument_matcher(matcher: &ArgumentMatcher) -> String {
    match matcher.kind() {
        value::ArgumentMatcherKind::Value(value) => explain_value_matcher(value),
        value::ArgumentMatcherKind::ObjectKeys(keys) => {
            format!("an object with keys {}", quoted_list(keys))
        }
        value::ArgumentMatcherKind::RootedExpressions(paths) => {
            format!("one of the rooted expressions {}", quoted_list(paths))
        }
        value::ArgumentMatcherKind::ObjectPropertyValue { property, value } => format!(
            "an object whose `{property}` property matches {}",
            explain_value_matcher(value)
        ),
    }
}

fn explain_value_matcher(matcher: &ValueMatcher) -> String {
    match matcher.kind() {
        value::ValueMatcherKind::Any => "any value".into(),
        value::ValueMatcherKind::StaticString(predicate) => match predicate.kind() {
            value::StaticStringPredicateKind::Any => "any static string".into(),
            value::StaticStringPredicateKind::Exact(values) => {
                format!("one of the exact strings {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::Prefix(values) => {
                format!("a string starting with one of {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::ContainsAny(values) => {
                format!("a string containing any of {}", quoted_list(values))
            }
            value::StaticStringPredicateKind::ContainsAll(values) => {
                format!("a string containing all of {}", quoted_list(values))
            }
        },
    }
}

fn quoted_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn explain_lifecycle(lifecycle: &LifecycleQuery) -> String {
    let sources = lifecycle
        .sources()
        .iter()
        .map(explain_event)
        .collect::<Vec<_>>()
        .join("; ");
    let condition = lifecycle.condition().map_or_else(
        || "no configuration condition".into(),
        explain_lifecycle_condition,
    );
    let completion = lifecycle.completion().map_or_else(
        || "no completion condition".into(),
        explain_lifecycle_completion,
    );
    format!(
        "a lifecycle object produced by {sources}; it requires {condition}; it completes when {completion}"
    )
}

fn explain_lifecycle_condition(condition: &lifecycle::LifecycleCondition) -> String {
    let (join, events) = match condition.kind() {
        lifecycle::LifecycleConditionKind::AnyOf(events) => ("any of", events),
        lifecycle::LifecycleConditionKind::AllOf(events) => ("all of", events),
    };
    format!(
        "{join} {}",
        events
            .iter()
            .map(explain_lifecycle_event)
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn explain_lifecycle_event(event: &lifecycle::LifecycleEvent) -> String {
    match event.kind() {
        lifecycle::LifecycleEventKind::PropertyWrite { property, value } => format!(
            "a write to `{property}` matching {}",
            explain_value_matcher(value)
        ),
        lifecycle::LifecycleEventKind::MemberCall { member, arguments } => {
            append_constraints(format!("a member call to `{}`", member.as_str()), arguments)
        }
    }
}

fn explain_lifecycle_completion(completion: &lifecycle::LifecycleCompletion) -> String {
    match completion.kind() {
        lifecycle::LifecycleCompletionKind::Configuration => "the configuration condition".into(),
        lifecycle::LifecycleCompletionKind::AnySink(sinks) => format!(
            "any sink {} receives the object",
            sinks
                .iter()
                .map(explain_lifecycle_sink)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        lifecycle::LifecycleCompletionKind::AllSinks(sinks) => format!(
            "all sinks {} receive the object",
            sinks
                .iter()
                .map(explain_lifecycle_sink)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn explain_lifecycle_sink(sink: &lifecycle::LifecycleSink) -> String {
    match sink.kind() {
        lifecycle::LifecycleSinkKind::ArgumentOf { endpoint, index } => {
            format!("`{}` argument {}", endpoint.chain(), index.get())
        }
        lifecycle::LifecycleSinkKind::AnyArgumentOf { endpoint } => {
            format!("any argument of `{}`", endpoint.chain())
        }
    }
}
