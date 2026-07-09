use glass_lint_core::rules::{
    Confidence, FlowValueMatcher, Rule, Rule as ApiRule, Severity as ApiSeverity, ValueFlowMatcher,
};

pub(super) fn rules() -> Vec<Rule> {
    vec![
        ApiRule::builder("network.browser")
            .label("Uses browser network APIs")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .global_calls(["fetch"])
            .rooted_member_calls(["navigator.sendBeacon"])
            .constructors(["XMLHttpRequest", "WebSocket", "EventSource"])
            .implies(["disclosure.network_access"])
            .build(),
        ApiRule::builder("network.obsidian")
            .label("Uses Obsidian request APIs")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .module_calls("obsidian", ["request", "requestUrl"])
            .module_member_calls("obsidian", ["request", "requestUrl"])
            .implies([
                "disclosure.network_access",
                "disclosure.cors_free_network_access",
            ])
            .build(),
        ApiRule::builder("network.node")
            .label("Uses Node HTTP modules")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .imports(["http", "https", "node:http", "node:https"])
            .implies(["disclosure.network_access"])
            .build(),
        ApiRule::builder("network.url_construction")
            .label("References URLs or constructs URL objects")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .constructors(["URL", "URLSearchParams"])
            .string_literals(["http://", "https://"])
            .build(),
        ApiRule::builder("network.private")
            .label("References localhost or private-network addresses")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::Medium)
            .string_literals([
                "localhost",
                "127.0.0.1",
                "0.0.0.0",
                "http://192.168.",
                "https://192.168.",
                "http://10.",
                "https://10.",
                "http://172.16.",
                "https://172.16.",
                "http://172.17.",
                "https://172.17.",
                "http://172.18.",
                "https://172.18.",
                "http://172.19.",
                "https://172.19.",
                "http://172.20.",
                "https://172.20.",
                "http://172.21.",
                "https://172.21.",
                "http://172.22.",
                "https://172.22.",
                "http://172.23.",
                "https://172.23.",
                "http://172.24.",
                "https://172.24.",
                "http://172.25.",
                "https://172.25.",
                "http://172.26.",
                "https://172.26.",
                "http://172.27.",
                "https://172.27.",
                "http://172.28.",
                "https://172.28.",
                "http://172.29.",
                "https://172.29.",
                "http://172.30.",
                "https://172.30.",
                "http://172.31.",
                "https://172.31.",
            ])
            .implies(["disclosure.private_network_access"])
            .build(),
        ApiRule::builder("network.ai_provider")
            .label("References AI provider endpoints or SDKs")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .imports([
                "openai",
                "@anthropic-ai/sdk",
                "@google/generative-ai",
                "@google/genai",
                "ollama",
                "replicate",
                "@huggingface/inference",
            ])
            .string_literals([
                "api.openai.com",
                "anthropic.com",
                "generativelanguage.googleapis.com",
                "openrouter.ai",
                "replicate.com",
                "huggingface.co",
                "localhost:11434",
            ])
            .implies([
                "disclosure.network_access",
                "disclosure.third_party_services",
            ])
            .build(),
        ApiRule::builder("network.sync_storage_provider")
            .label("References sync or storage provider endpoints or SDKs")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .imports([
                "@supabase/supabase-js",
                "firebase",
                "firebase-admin",
                "dropbox",
                "@notionhq/client",
                "aws-sdk",
                "@aws-sdk/client-s3",
            ])
            .string_literals([
                "api.github.com",
                "gitlab.com",
                "dropboxapi.com",
                "googleapis.com/drive",
                "graph.microsoft.com",
                "amazonaws.com",
                "supabase.co",
                "firebaseio.com",
                "firestore.googleapis.com",
                "api.notion.com",
                "api.airtable.com",
                "api.todoist.com",
                "api.telegram.org",
                "discord.com/api",
                "hooks.slack.com",
            ])
            .implies([
                "disclosure.network_access",
                "disclosure.third_party_services",
            ])
            .build(),
        ApiRule::builder("network.telemetry")
            .label("References telemetry or analytics SDKs")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .imports([
                "@sentry/browser",
                "@sentry/node",
                "posthog-js",
                "mixpanel-browser",
                "analytics",
                "@segment/analytics-node",
                "@datadog/browser-rum",
            ])
            .string_literals([
                "sentry.io",
                "app.posthog.com",
                "us.i.posthog.com",
                "eu.i.posthog.com",
                "plausible.io",
                "google-analytics.com",
                "googletagmanager.com",
                "mixpanel.com",
                "segment.com",
                "amplitude.com",
                "datadoghq.com",
            ])
            .implies([
                "disclosure.network_access",
                "disclosure.telemetry_or_error_reporting",
            ])
            .build(),
        ApiRule::builder("network.headers")
            .label("References user-agent or authorization headers")
            .category("network")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .string_literals(["User-Agent", "user-agent", "Authorization"])
            .build(),
        ApiRule::builder("network.remote_dom_loading")
            .label("Loads remote image, script, or style elements")
            .category("network")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::Medium)
            .member_calls(["appendChild", "append"])
            .matcher(remote_dom_element_flow(
                "remote script element",
                "script",
                "src",
                remote_url_prefixes(),
            ))
            .matcher(remote_dom_element_flow(
                "remote image element",
                "img",
                "src",
                remote_url_prefixes(),
            ))
            .matcher(
                ValueFlowMatcher::new("remote stylesheet link".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["link"])
                    .property_write(
                        "rel",
                        FlowValueMatcher::StaticExact(vec!["stylesheet".to_string()]),
                    )
                    .property_write("href", remote_url_prefixes())
                    .require_all_configurations()
                    .sink_member_call_arg_indices(dom_insertion_indexed_sinks(), [0])
                    .sink_member_call_any_arg(dom_insertion_any_arg_sinks()),
            )
            .matcher(
                ValueFlowMatcher::new("remote style element".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["style"])
                    .property_write("textContent", remote_url_markers())
                    .sink_member_call_arg_indices(dom_insertion_indexed_sinks(), [0])
                    .sink_member_call_any_arg(dom_insertion_any_arg_sinks()),
            )
            .implies(["disclosure.network_access"])
            .build(),
    ]
    .into_iter()
    .map(|rule| rule.expect("built-in Obsidian rule should be valid"))
    .collect()
}

fn remote_dom_element_flow(
    symbol: &str,
    tag: &str,
    url_property: &str,
    url_matcher: FlowValueMatcher,
) -> ValueFlowMatcher {
    ValueFlowMatcher::new(symbol.to_string())
        .source_member_call("document.createElement")
        .source_arg_string(0, [tag])
        .property_write(url_property, url_matcher.clone())
        .member_call_config(
            "setAttribute",
            [
                (
                    0,
                    FlowValueMatcher::StaticExact(vec![url_property.to_string()]),
                ),
                (1, url_matcher),
            ],
        )
        .sink_member_call_arg_indices(dom_insertion_indexed_sinks(), [0])
        .sink_member_call_any_arg(dom_insertion_any_arg_sinks())
}

fn dom_insertion_indexed_sinks() -> [&'static str; 4] {
    [
        "document.head.appendChild",
        "document.body.appendChild",
        "document.documentElement.appendChild",
        "document.documentElement.insertBefore",
    ]
}

fn dom_insertion_any_arg_sinks() -> [&'static str; 5] {
    [
        "document.head.append",
        "document.body.append",
        "document.body.prepend",
        "document.documentElement.append",
        "document.documentElement.prepend",
    ]
}

fn remote_url_prefixes() -> FlowValueMatcher {
    FlowValueMatcher::StaticPrefix(vec![
        "http://".to_string(),
        "https://".to_string(),
        "//".to_string(),
    ])
}

fn remote_url_markers() -> FlowValueMatcher {
    FlowValueMatcher::StaticContainsAny(vec![
        "http://".to_string(),
        "https://".to_string(),
        "//".to_string(),
    ])
}
