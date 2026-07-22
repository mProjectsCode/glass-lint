//! Browser remote-DOM-resource flow rule definition.

use glass_lint_core::rules::{
    Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, MatcherDecl, ObjectEventMatcher,
    ObjectFlowMatcher, ObjectSourceMatcher, Rule, Severity, ValueMatcher,
};

/// Detects a script or image created by `document.createElement`, configured
/// with a static remote `src` via assignment or `setAttribute`, then passed to
/// a supported DOM insertion sink. Direct aliases participate in the bounded
/// object flow; local paths, dynamic values, other tags, and unsupported sinks
/// do not match.
pub fn rule() -> Rule {
    Rule::builder("dom.remote-resource")
        .description("Loads remote DOM resources")
        .category("browser/dom")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote script element",
            "script",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote image element",
            "img",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote link element",
            "link",
            "href",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote iframe element",
            "iframe",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote audio element",
            "audio",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote video element",
            "video",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote source element",
            "source",
            "src",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote object element",
            "object",
            "data",
        )))
        .declaration(MatcherDecl::from_object_flow(&remote_element_flow(
            "remote embed element",
            "embed",
            "src",
        )))
        .build()
        .unwrap()
}

fn remote_element_flow(symbol: &str, tag: &str, property: &str) -> ObjectFlowMatcher {
    // Both property assignment and setAttribute configure the same bounded
    // element flow; the static URL prefix keeps the heuristic intentionally
    // narrow and avoids treating local or dynamic sources as remote.
    let remote_url = ValueMatcher::static_string().starts_with_any(["http://", "https://", "//"]);
    ObjectFlowMatcher::builder(symbol)
        .source(
            ObjectSourceMatcher::returned_by("document.createElement")
                .arg(0, ValueMatcher::static_string().equals(tag)),
        )
        .configured_by(FlowCondition::any_of([
            ObjectEventMatcher::property_write(property, remote_url.clone()),
            ObjectEventMatcher::member_call("setAttribute")
                .arg(0, ValueMatcher::static_string().equals(property))
                .arg(1, remote_url)
                .build(),
        ]))
        .complete_at(FlowCompletion::any_sink(
            [
                "document.head.appendChild",
                "document.body.appendChild",
                "document.documentElement.appendChild",
                "document.documentElement.insertBefore",
            ]
            .into_iter()
            .map(|chain| FlowSinkMatcher::argument_of(chain, 0))
            .chain(
                [
                    "document.head.append",
                    "document.body.append",
                    "document.body.prepend",
                    "document.documentElement.append",
                    "document.documentElement.prepend",
                ]
                .into_iter()
                .map(FlowSinkMatcher::any_argument_of),
            ),
        ))
        .build()
}
