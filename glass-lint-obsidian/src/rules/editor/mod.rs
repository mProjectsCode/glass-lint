//! Obsidian editor-integration rule catalog.

mod content;
mod extension;
mod suggest;
use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    vec![content::rule(), extension::rule(), suggest::rule()]
}
