use glass_lint_core::rules::{Confidence, FlowMatcher, FlowValueMatcher, Matcher, Rule, Severity};

/// Detects a script or image created by `document.createElement`, configured
/// with a static remote `src` via assignment or `setAttribute`, then passed to
/// a supported DOM insertion sink. Direct aliases participate in the bounded
/// object flow; local paths, dynamic values, other tags, and unsupported sinks
/// do not match.
pub(crate) fn rule() -> Rule {
    Rule::builder("dom.remote-resource")
        .label("Loads remote DOM resources")
        .category("browser/dom")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::flow(remote_element_flow(
            "remote script element",
            "script",
            "src",
        )))
        .matcher(Matcher::flow(remote_element_flow(
            "remote image element",
            "img",
            "src",
        )))
        .build()
        .unwrap()
}

fn remote_element_flow(symbol: &str, tag: &str, property: &str) -> FlowMatcher {
    FlowMatcher::new(symbol)
        .source_member_call("document.createElement")
        .source_arg_string(0, [tag])
        .property_write(
            property,
            FlowValueMatcher::StaticPrefix(vec![
                "http://".to_string(),
                "https://".to_string(),
                "//".to_string(),
            ]),
        )
        .member_call_config(
            "setAttribute",
            [
                (0, FlowValueMatcher::StaticExact(vec![property.to_string()])),
                (
                    1,
                    FlowValueMatcher::StaticPrefix(vec![
                        "http://".to_string(),
                        "https://".to_string(),
                        "//".to_string(),
                    ]),
                ),
            ],
        )
        .sink_member_call_arg_indices(
            [
                "document.head.appendChild",
                "document.body.appendChild",
                "document.documentElement.appendChild",
                "document.documentElement.insertBefore",
            ],
            [0],
        )
        .sink_member_call_any_arg([
            "document.head.append",
            "document.body.append",
            "document.body.prepend",
            "document.documentElement.append",
            "document.documentElement.prepend",
        ])
}
