//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals and template quasis, so they do not
/// prove network use or reconstruct arbitrary concatenated or dynamic values.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .description("References service or SDK endpoints")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::import_package("openai"))
        .query(QueryDecl::import_package("firebase"))
        .query(QueryDecl::import_package("dropbox"))
        .query(QueryDecl::import_package("@supabase/supabase-js"))
        .query(QueryDecl::import_package("@aws-sdk/client-s3"))
        .query(QueryDecl::import_package("@aws-sdk/client-dynamodb"))
        .query(QueryDecl::import_package("@aws-sdk/client-lambda"))
        .query(QueryDecl::import_package("@google-cloud/storage"))
        .query(QueryDecl::import_package("@google-cloud/firestore"))
        .query(QueryDecl::import_package("@google-cloud/pubsub"))
        .query(QueryDecl::import_package("@azure/storage-blob"))
        .query(QueryDecl::import_package("@azure/identity"))
        .query(QueryDecl::import_package("stripe"))
        .query(QueryDecl::import_package("@stripe/stripe-js"))
        .query(QueryDecl::import_package("twilio"))
        .query(QueryDecl::import_package("@twilio/voice-sdk"))
        .query(QueryDecl::import_package("@sendgrid/mail"))
        .query(QueryDecl::import_package("mailgun.js"))
        .query(QueryDecl::import_package("@octokit/rest"))
        .query(QueryDecl::string_contains("api.openai.com"))
        .query(QueryDecl::string_contains("amazonaws.com"))
        .query(QueryDecl::string_contains("supabase.co"))
        .query(QueryDecl::string_contains("api.stripe.com"))
        .query(QueryDecl::string_contains("api.twilio.com"))
        .query(QueryDecl::string_contains("api.sendgrid.com"))
        .query(QueryDecl::string_contains("api.mailgun.net"))
        .query(QueryDecl::string_contains("slack.com/api"))
        .build()
        .unwrap()
}
