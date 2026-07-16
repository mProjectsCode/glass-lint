//! Obsidian network rule catalog.

mod request;
use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    vec![request::rule()]
}
