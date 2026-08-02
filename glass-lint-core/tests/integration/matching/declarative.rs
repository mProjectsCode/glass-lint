//! Declarative matcher behavior exercised through the public provider API.
//!
//! The helpers intentionally build a new catalog per case so rule selection,
//! environment configuration, and finding counts remain independently visible.

#[allow(unused_imports)]
use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    project::SourceFile,
    rules::{
        ArgumentMatcher, EventQuery, LifecycleCompletion, LifecycleCondition, LifecycleEvent,
        LifecycleQuery, LifecycleSink, QueryDecl, ValueMatcher,
    },
};

mod arguments;
mod flow;
mod globals;
mod lifecycle;

#[allow(unused_imports)]
use crate::support::{self, Classification, classify, classify_with_environment, rule};

fn source(path: &str, text: &str) -> SourceFile {
    SourceFile::new(path, text).unwrap()
}

/// Construct the multi-step flow used by source/configuration/sink tests.
fn script_insertion_flow() -> LifecycleQuery {
    LifecycleQuery::builder("script insertion")
        .source(
            EventQuery::member_call_rooted("document.createElement")
                .unwrap()
                .with_arg(0, ValueMatcher::static_string().equals("script")),
        )
        .condition(LifecycleCondition::event(LifecycleEvent::property_write(
            "src",
            ValueMatcher::any_value(),
        )))
        .completion(LifecycleCompletion::any_sink([
            LifecycleSink::argument_of_member("document.head.appendChild", 0),
        ]))
        .build()
        .unwrap()
}

/// Require both the named capability and the exact total finding count.
fn assert_capability_count(result: &Classification, id: &str, expected: usize) {
    assert!(result.has_capability(id));
    assert_eq!(result.finding_count, expected);
}
