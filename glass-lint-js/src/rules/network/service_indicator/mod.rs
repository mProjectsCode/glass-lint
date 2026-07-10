use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .label("References service or SDK endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("openai"))
        .matcher(Matcher::import("firebase"))
        .matcher(Matcher::import("dropbox"))
        .matcher(Matcher::import("@supabase/supabase-js"))
        .matcher(Matcher::string_literal("api.openai.com"))
        .matcher(Matcher::string_literal("amazonaws.com"))
        .matcher(Matcher::string_literal("supabase.co"))
        .build()
        .unwrap()
}
