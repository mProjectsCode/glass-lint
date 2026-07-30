//! Browser remote-DOM-resource flow rule definition.

use glass_lint_core::rules::{
    Category, Confidence, LifecycleCompletion, LifecycleCondition, LifecycleEvent, LifecycleQuery,
    LifecycleSink, LifecycleSource, QueryBuildError, QueryDecl, Rule, Severity, ValueMatcher,
};

/// Detects a script or image created by `document.createElement`, configured
/// with a static remote `src` via assignment or `setAttribute`, then passed to
/// a supported DOM insertion sink. Direct aliases participate in the bounded
/// object flow; local paths, dynamic values, other tags, and unsupported sinks
/// do not match.
pub fn rule() -> Rule {
    Rule::builder("dom.remote-resource")
        .description("Loads remote DOM resources")
        .category(Category::new("browser/dom").unwrap())
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote script element",
            "script",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote image element",
            "img",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote link element",
            "link",
            "href",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote iframe element",
            "iframe",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote audio element",
            "audio",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote video element",
            "video",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote source element",
            "source",
            "src",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote object element",
            "object",
            "data",
        )))
        .query(QueryDecl::lifecycle(remote_element_query(
            "remote embed element",
            "embed",
            "src",
        )))
        .build()
        .unwrap()
}

fn remote_element_query(
    symbol: &str,
    tag: &str,
    property: &str,
) -> Result<LifecycleQuery, QueryBuildError> {
    let remote_url = ValueMatcher::static_string()
        .starts_with_any(["http://", "https://", "//"])
        .unwrap();
    LifecycleQuery::builder(symbol)
        .source(
            LifecycleSource::returned_by("document.createElement")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals(tag)),
        )
        .condition(LifecycleCondition::any_of([
            LifecycleEvent::property_write(property, remote_url.clone()),
            Ok(LifecycleEvent::member_call("setAttribute")
                .unwrap()
                .arg(0, ValueMatcher::static_string().equals(property))
                .unwrap()
                .arg(1, remote_url)
                .unwrap()
                .build()),
        ]))
        .completion(LifecycleCompletion::any_sink(
            [
                "document.head.appendChild",
                "document.body.appendChild",
                "document.documentElement.appendChild",
                "document.documentElement.insertBefore",
            ]
            .into_iter()
            .map(|chain| LifecycleSink::argument_of(chain, 0))
            .chain(
                [
                    "document.head.append",
                    "document.body.append",
                    "document.body.prepend",
                    "document.documentElement.append",
                    "document.documentElement.prepend",
                ]
                .into_iter()
                .map(LifecycleSink::any_argument_of),
            ),
        ))
        .build()
}
