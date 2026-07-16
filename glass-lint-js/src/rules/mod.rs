//! Catalog assembly for browser, Electron, general JavaScript, and Node rules.
//!
//! Subcatalogs own rule definitions; this module fixes their provider-wide
//! ordering so metadata and finding order remain deterministic.

mod browser;
mod electron;
mod general;
mod node;

use glass_lint_core::rules::Rule;

pub fn all() -> Vec<Rule> {
    // Keep category order explicit rather than relying on filesystem/module
    // discovery, which would make catalogs less reproducible.
    [
        browser::rules(),
        electron::rules(),
        general::rules(),
        node::rules(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
