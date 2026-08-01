//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals, template quasis, and bounded constant
/// compositions, so they do not prove network use or reconstruct arbitrary
/// dynamic values.
/// This is intentionally a low-confidence dependency/literal indicator, not
/// an operation witness.
const SERVICE_PACKAGES: &[&str] = &[
    "openai",
    "firebase",
    "dropbox",
    "@supabase/supabase-js",
    "@aws-sdk/client-s3",
    "@aws-sdk/client-dynamodb",
    "@aws-sdk/client-lambda",
    "@google-cloud/storage",
    "@google-cloud/firestore",
    "@google-cloud/pubsub",
    "@azure/storage-blob",
    "@azure/identity",
    "stripe",
    "@stripe/stripe-js",
    "twilio",
    "@twilio/voice-sdk",
    "@sendgrid/mail",
    "mailgun.js",
    "@octokit/rest",
];

const SERVICE_ENDPOINTS: &[&str] = &[
    "api.openai.com",
    "amazonaws.com",
    "supabase.co",
    "api.stripe.com",
    "api.twilio.com",
    "api.sendgrid.com",
    "api.mailgun.net",
    "slack.com/api",
];

pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .description("References service or SDK endpoints")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Low)
        .queries(
            SERVICE_PACKAGES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .queries(
            SERVICE_ENDPOINTS
                .iter()
                .copied()
                .map(EventQuery::string_contains),
        )
        .build()
        .unwrap()
}
