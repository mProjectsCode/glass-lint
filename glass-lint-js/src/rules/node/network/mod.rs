//! Node HTTP-module rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the configured Node
/// network modules and exact client packages. It reports the module load
/// itself, not later API use, and relies on module provenance so similar names
/// and shadowed `require` bindings are excluded.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("node.network")
        .description("Uses Node HTTP modules")
        .category(Category::new("node/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::import_exact("http"))
        .query(EventQuery::import_exact("https"))
        .query(EventQuery::import_exact("node:http"))
        .query(EventQuery::import_exact("node:https"))
        .query(EventQuery::import_exact("http2"))
        .query(EventQuery::import_exact("node:http2"))
        .query(EventQuery::import_exact("net"))
        .query(EventQuery::import_exact("node:net"))
        .query(EventQuery::import_exact("tls"))
        .query(EventQuery::import_exact("node:tls"))
        .query(EventQuery::import_exact("dgram"))
        .query(EventQuery::import_exact("node:dgram"))
        .query(EventQuery::import_exact("dns"))
        .query(EventQuery::import_exact("node:dns"))
        .query(EventQuery::import_exact("dns/promises"))
        .query(EventQuery::import_exact("node:dns/promises"))
        .query(EventQuery::import_package("undici"))
        .query(EventQuery::import_package("axios"))
        .query(EventQuery::import_package("node-fetch"))
        .query(EventQuery::import_package("got"))
        .query(EventQuery::import_package("superagent"))
        .query(EventQuery::import_package("ws"))
        .query(EventQuery::import_package("cross-fetch"))
        .query(EventQuery::import_package("ky"))
        .query(EventQuery::import_package("graphql-request"))
        .query(EventQuery::import_package("request"))
        .query(EventQuery::import_package("needle"))
        .query(EventQuery::import_package("@grpc/grpc-js"))
        .query(EventQuery::import_package("@apollo/client"))
        .query(EventQuery::import_package("graphql"))
        .query(EventQuery::import_package("@elastic/elasticsearch"))
        .query(EventQuery::import_package("fetch-retry"))
        .query(EventQuery::import_package("fetch-blob"))
        .query(EventQuery::import_package("form-data"))
        .query(EventQuery::import_package("http-proxy"))
        .query(EventQuery::import_package("http-proxy-agent"))
        .query(EventQuery::import_package("https-proxy-agent"))
        .query(EventQuery::import_package("socks-proxy-agent"))
        .query(EventQuery::import_package("@whatwg-node/fetch"))
        .build()
        .unwrap()
}
