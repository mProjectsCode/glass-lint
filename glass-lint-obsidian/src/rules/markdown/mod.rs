//! Obsidian markdown rule catalog.
//!
//! Registration rules use proven plugin instances, while renderer and link
//! rules retain their explicitly documented heuristic or module boundaries.

mod code_block_processor;
mod link;
mod postprocessor;
mod render;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    // Keep registration rules before rendering/link helpers for stable catalog
    // metadata and deterministic finding order.
    vec![
        postprocessor::rule(),
        code_block_processor::rule(),
        render::rule(),
        link::rule(),
    ]
}
