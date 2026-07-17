//! Catalog assembly for browser, Electron, general JavaScript, and Node rules.
//!
//! Subcatalogs own rule definitions; this module fixes their provider-wide
//! ordering so metadata and finding order remain deterministic.

mod browser;
mod electron;
mod js;
mod node;

use glass_lint_core::rules::Rule;

pub fn js() -> Vec<Rule> {
    js::rules()
}
pub fn browser() -> Vec<Rule> {
    browser::rules()
}
pub fn electron() -> Vec<Rule> {
    electron::rules()
}
pub fn node() -> Vec<Rule> {
    node::rules()
}
