use std::sync::OnceLock;

use glass_lint_core::rules::Rule;

mod content;
mod interface;
mod network;
mod system;

pub(crate) fn obsidian_api_rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            network::rules(),
            content::rules(),
            interface::rules(),
            system::rules(),
        ]
        .into_iter()
        .flatten()
        .collect()
    })
}
