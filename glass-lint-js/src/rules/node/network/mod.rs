//! Node HTTP-module rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the configured Node
/// network modules and exact client packages. It reports the module load
/// itself, not later API use, and relies on module provenance so similar names
/// and shadowed `require` bindings are excluded.
const EXACT_MODULES: &[&str] = &[
    "http",
    "https",
    "node:http",
    "node:https",
    "http2",
    "node:http2",
    "net",
    "node:net",
    "tls",
    "node:tls",
    "dgram",
    "node:dgram",
    "dns",
    "node:dns",
    "dns/promises",
    "node:dns/promises",
];

const PACKAGE_MODULES: &[&str] = &[
    "undici",
    "axios",
    "node-fetch",
    "got",
    "superagent",
    "ws",
    "cross-fetch",
    "ky",
    "graphql-request",
    "request",
    "needle",
    "@grpc/grpc-js",
    "@apollo/client",
    "graphql",
    "@elastic/elasticsearch",
    "fetch-retry",
    "fetch-blob",
    "form-data",
    "http-proxy",
    "http-proxy-agent",
    "https-proxy-agent",
    "socks-proxy-agent",
    "@whatwg-node/fetch",
];

pub fn rule() -> Rule {
    Rule::builder("node.network")
        .description("Uses Node HTTP modules")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .queries(EXACT_MODULES.iter().copied().map(EventQuery::import_exact))
        .queries(
            PACKAGE_MODULES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .build()
        .unwrap()
}
