//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals and template quasis, so they do not
/// prove network use or reconstruct arbitrary concatenated or dynamic values.
/// This is intentionally a low-confidence dependency/literal indicator, not
/// an operation witness.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .description("References service or SDK endpoints")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Low)
        .query(EventQuery::import_package("openai"))
        .query(EventQuery::import_package("firebase"))
        .query(EventQuery::import_package("dropbox"))
        .query(EventQuery::import_package("@supabase/supabase-js"))
        .query(EventQuery::import_package("@aws-sdk/client-s3"))
        .query(EventQuery::import_package("@aws-sdk/client-dynamodb"))
        .query(EventQuery::import_package("@aws-sdk/client-lambda"))
        .query(EventQuery::import_package("@google-cloud/storage"))
        .query(EventQuery::import_package("@google-cloud/firestore"))
        .query(EventQuery::import_package("@google-cloud/pubsub"))
        .query(EventQuery::import_package("@azure/storage-blob"))
        .query(EventQuery::import_package("@azure/identity"))
        .query(EventQuery::import_package("stripe"))
        .query(EventQuery::import_package("@stripe/stripe-js"))
        .query(EventQuery::import_package("twilio"))
        .query(EventQuery::import_package("@twilio/voice-sdk"))
        .query(EventQuery::import_package("@sendgrid/mail"))
        .query(EventQuery::import_package("mailgun.js"))
        .query(EventQuery::import_package("@octokit/rest"))
        .query(EventQuery::string_contains("api.openai.com"))
        .query(EventQuery::string_contains("amazonaws.com"))
        .query(EventQuery::string_contains("supabase.co"))
        .query(EventQuery::string_contains("api.stripe.com"))
        .query(EventQuery::string_contains("api.twilio.com"))
        .query(EventQuery::string_contains("api.sendgrid.com"))
        .query(EventQuery::string_contains("api.mailgun.net"))
        .query(EventQuery::string_contains("slack.com/api"))
        .build()
        .unwrap()
}
