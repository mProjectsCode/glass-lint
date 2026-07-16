//! Obsidian lifecycle rule catalog.

mod events;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![events::rule()]
}
