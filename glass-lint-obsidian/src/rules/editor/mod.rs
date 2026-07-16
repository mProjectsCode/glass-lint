//! Obsidian editor-integration rule catalog.

mod extension;
mod suggest;
use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    vec![extension::rule(), suggest::rule()]
}
