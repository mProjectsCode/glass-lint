use std::sync::OnceLock;

use glass_lint_core::rules::Rule;

mod disclosures;

pub(crate) fn obsidian_api_rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(crate::rules::all)
}

pub(crate) fn disclosures_for_rule(id: &str) -> &'static [&'static str] {
    disclosures::for_rule(id)
}
