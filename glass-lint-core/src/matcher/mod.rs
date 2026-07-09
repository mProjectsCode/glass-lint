//! Provenance-aware, declarative JavaScript API matching.

use swc_ecma_ast::Program;

mod result;
mod rule;
mod symbol_index;

#[cfg(test)]
mod minified_tests;

pub use result::{ApiCapability, ApiClassificationResult, Disclosure};
pub use rule::{
    ApiCatalogError, ApiCategory, ApiRule, ApiRuleBuildError, ApiSeverity, CallMatcher,
    ClassMatcher, Confidence, ConstructorMatcher, FlowValueMatcher, Matcher, MemberCallMatcher,
    MemberReadMatcher, ValueFlowMatcher,
};

use symbol_index::SymbolIndex;

/// Classifies a parsed program with caller-provided rules. Core owns no catalog.
pub fn classify_api_usage(program: Option<&Program>, rules: &[ApiRule]) -> ApiClassificationResult {
    let aliases = program
        .map(symbol_index::AliasInfo::collect)
        .unwrap_or_default();
    let (symbol_index, argument_evidence) =
        SymbolIndex::collect_for_rules(program, &aliases, rules);
    let mut result = ApiClassificationResult::default();

    for (index, rule) in rules.iter().enumerate() {
        let mut evidence = symbol_index.evidence_for(rule);
        evidence.extend_from_slice(&argument_evidence[index]);
        evidence.truncate(ApiRule::EVIDENCE_LIMIT);
        if evidence.is_empty() {
            continue;
        }

        result.capabilities.push(ApiCapability {
            id: rule.id.clone(),
            label: rule.label.clone(),
            category: rule.category.clone(),
            severity: rule.severity,
            confidence: rule.confidence,
            evidence,
        });
        result
            .disclosures
            .extend(rule.implies.iter().map(|id| Disclosure {
                id: id.clone(),
                from_capability: rule.id.clone(),
            }));
    }

    result
}

/// Validates catalog-wide invariants that are independent of a provider.
pub fn validate_catalog(rules: &[ApiRule]) -> Result<(), ApiCatalogError> {
    let mut ids = std::collections::BTreeSet::new();
    for rule in rules {
        if !ids.insert(rule.id.clone()) {
            return Err(ApiCatalogError::DuplicateRule(rule.id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str) -> rule::ApiRuleBuilder {
        ApiRule::builder(id)
            .label(id)
            .category("test")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
    }

    fn classify(source: &str, rules: &[ApiRule]) -> ApiClassificationResult {
        let parsed = crate::parse(source, "input.js").unwrap();
        classify_api_usage(Some(&parsed.program), rules)
    }

    fn evidence_count(result: &ApiClassificationResult, id: &str) -> u32 {
        result
            .capabilities()
            .iter()
            .find(|capability| capability.id() == id)
            .map(|capability| {
                capability
                    .evidence()
                    .iter()
                    .map(|evidence| evidence.count())
                    .sum()
            })
            .unwrap_or(0)
    }

    #[test]
    fn resolves_module_provenance_and_rejects_local_lookalikes() {
        let rules = [rule("test.module")
            .module_calls("example-sdk", ["send"])
            .build()
            .unwrap()];
        let result = classify(
            "import { send as sdkSend } from 'example-sdk'; sdkSend(); function send() {} send();",
            &rules,
        );
        assert!(result.has_capability("test.module"));
        assert_eq!(evidence_count(&result, "test.module"), 1);
    }

    #[test]
    fn resolves_commonjs_destructured_module_exports() {
        let rules = [rule("test.module")
            .module_calls("example-sdk", ["send"])
            .build()
            .unwrap()];
        let result = classify(
            "const { send: sdkSend } = require('example-sdk'); sdkSend();",
            &rules,
        );
        assert!(result.has_capability("test.module"));
        assert_eq!(evidence_count(&result, "test.module"), 1);
    }

    #[test]
    fn follows_rooted_aliases_and_reassignment_order() {
        let rules = [rule("test.alias")
            .rooted_member_calls(["host.files.read"])
            .build()
            .unwrap()];
        let result = classify(
            "let files = host.files; files.read(); files = local; files.read();",
            &rules,
        );
        assert!(result.has_capability("test.alias"));
        assert_eq!(evidence_count(&result, "test.alias"), 1);
    }

    #[test]
    fn rejects_aliases_after_shadowing_reassignment() {
        let rules = [rule("test.fetch").global_calls(["fetch"]).build().unwrap()];
        let result = classify(
            "let send = fetch; send('/remote'); send = localFetch; send('/local');",
            &rules,
        );
        assert!(result.has_capability("test.fetch"));
        assert_eq!(evidence_count(&result, "test.fetch"), 1);
    }

    #[test]
    fn matches_static_string_arguments_but_rejects_dynamic_strings() {
        let rules = [rule("test.fetch-url")
            .matcher(CallMatcher::global("fetch").static_string_arg(0))
            .build()
            .unwrap()];
        let result = classify("fetch('/literal'); fetch('/' + dynamic);", &rules);
        assert!(result.has_capability("test.fetch-url"));
        assert_eq!(evidence_count(&result, "test.fetch-url"), 1);
    }

    #[test]
    fn tracks_rooted_expression_arguments_through_aliases() {
        let rules = [rule("test.arg-flow")
            .matcher(
                MemberCallMatcher::rooted_chain("app.open").arg_rooted_exprs(0, ["vault.file"]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const file = vault.file; const opener = app; opener.open(file);",
            &rules,
        );
        assert!(result.has_capability("test.arg-flow"));
        assert_eq!(evidence_count(&result, "test.arg-flow"), 1);
    }

    #[test]
    fn tracks_simple_parameter_aliases_into_named_functions() {
        let rules = [rule("test.fetch").global_calls(["fetch"]).build().unwrap()];
        let result = classify(
            "function invoke(callback) { callback('/remote'); } invoke(fetch);",
            &rules,
        );
        assert!(result.has_capability("test.fetch"));
        assert_eq!(evidence_count(&result, "test.fetch"), 1);
    }

    #[test]
    fn target_tracks_parameter_aliases_into_arrow_functions() {
        let rules = [rule("test.fetch").global_calls(["fetch"]).build().unwrap()];
        let result = classify(
            "const invoke = (callback) => callback('/remote'); invoke(fetch);",
            &rules,
        );
        assert!(result.has_capability("test.fetch"));
        assert_eq!(evidence_count(&result, "test.fetch"), 1);
    }

    #[test]
    fn target_matches_optional_chained_calls_with_static_arguments() {
        let rules = [rule("test.optional")
            .matcher(
                MemberCallMatcher::rooted_chain("app.commands.execute").arg_string(0, ["open"]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const commands = app.commands; commands?.execute?.('open');",
            &rules,
        );
        assert!(result.has_capability("test.optional"));
        assert_eq!(evidence_count(&result, "test.optional"), 1);
    }

    #[test]
    fn target_resolves_literal_computed_properties_through_constant_aliases() {
        let rules = [rule("test.computed")
            .rooted_member_calls(["window.fetch"])
            .build()
            .unwrap()];
        let result = classify("const method = 'fetch'; window[method]('/remote');", &rules);
        assert!(result.has_capability("test.computed"));
        assert_eq!(evidence_count(&result, "test.computed"), 1);
    }

    #[test]
    fn target_reuses_constant_object_arguments_for_key_matching() {
        let rules = [rule("test.object-arg")
            .matcher(
                MemberCallMatcher::rooted_chain("client.request")
                    .arg_object_keys(0, ["url", "method"]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const options = { url: '/remote', method: 'GET' }; client.request(options);",
            &rules,
        );
        assert!(result.has_capability("test.object-arg"));
        assert_eq!(evidence_count(&result, "test.object-arg"), 1);
    }

    #[test]
    fn tracks_configured_values_into_later_member_sinks() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("script insertion".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .property_write("src", FlowValueMatcher::Any)
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const script = document.createElement('script'); script.src = getUrl(); document.head.appendChild(script);",
            &rules,
        );
        assert!(result.has_capability("test.flow"));
        assert_eq!(evidence_count(&result, "test.flow"), 1);
    }

    #[test]
    fn value_flow_respects_reassignment_and_order() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("script insertion".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .property_write("src", FlowValueMatcher::Any)
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "let script = document.createElement('script'); script.src = getUrl(); script = document.createElement('div'); document.head.appendChild(script);
             const future = document.createElement('script'); document.head.appendChild(future); future.src = getUrl();",
            &rules,
        );
        assert_eq!(evidence_count(&result, "test.flow"), 0);
    }

    #[test]
    fn value_flow_supports_member_call_configuration_and_helper_sinks() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("script insertion".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .member_call_config(
                        "setAttribute",
                        [
                            (0, FlowValueMatcher::StaticExact(vec!["src".into()])),
                            (1, FlowValueMatcher::Any),
                        ],
                    )
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "function appendToHead(node) { document.head.appendChild(node); }
             const script = document.createElement('script'); script.setAttribute('src', getUrl()); appendToHead(script);",
            &rules,
        );
        assert!(result.has_capability("test.flow"));
        assert_eq!(evidence_count(&result, "test.flow"), 1);
    }

    #[test]
    fn value_flow_supports_const_arrow_helper_sinks() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("script insertion".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .property_write("src", FlowValueMatcher::Any)
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const appendToHead = node => document.head.appendChild(node);
             const script = document.createElement('script'); script.src = getUrl(); appendToHead(script);",
            &rules,
        );
        assert!(result.has_capability("test.flow"));
        assert_eq!(evidence_count(&result, "test.flow"), 1);
    }

    #[test]
    fn value_flow_static_prefix_requires_static_values() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("remote element".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["img"])
                    .property_write(
                        "src",
                        FlowValueMatcher::StaticPrefix(vec!["https://".into(), "http://".into()]),
                    )
                    .sink_member_call_arg_indices(["document.body.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const remote = document.createElement('img'); remote.src = 'https://example.com/a.png'; document.body.appendChild(remote);
             const local = document.createElement('img'); local.src = '/a.png'; document.body.appendChild(local);
             const dynamic = document.createElement('img'); dynamic.src = getUrl(); document.body.appendChild(dynamic);",
            &rules,
        );
        assert!(result.has_capability("test.flow"));
        assert_eq!(evidence_count(&result, "test.flow"), 1);
    }

    #[test]
    fn value_flow_can_require_all_configurations() {
        let rules = [rule("test.flow")
            .matcher(
                ValueFlowMatcher::new("remote stylesheet".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["link"])
                    .property_write(
                        "rel",
                        FlowValueMatcher::StaticExact(vec!["stylesheet".into()]),
                    )
                    .property_write(
                        "href",
                        FlowValueMatcher::StaticPrefix(vec!["https://".into()]),
                    )
                    .require_all_configurations()
                    .sink_member_call_arg_indices(["document.head.appendChild"], [0]),
            )
            .build()
            .unwrap()];
        let result = classify(
            "const good = document.createElement('link'); good.rel = 'stylesheet'; good.href = 'https://example.com/a.css'; document.head.appendChild(good);
             const missing = document.createElement('link'); missing.href = 'https://example.com/a.css'; document.head.appendChild(missing);",
            &rules,
        );
        assert!(result.has_capability("test.flow"));
        assert_eq!(evidence_count(&result, "test.flow"), 1);
    }
}
