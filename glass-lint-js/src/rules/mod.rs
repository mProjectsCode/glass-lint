mod browser;
mod electron;
mod network;
mod node;

use glass_lint_core::rules::Rule;

pub(crate) fn all() -> Vec<Rule> {
    [
        browser::rules(),
        electron::rules(),
        network::rules(),
        node::rules(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
