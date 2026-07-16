//! Obsidian metadata rule catalog.
//!
//! The child rules separate cache access, event registration, extraction, and
//! traversal so each finding retains a precise semantic boundary.

mod cache_read;
mod events;
mod extract;
mod frontmatter_read;
mod traversal;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    // Keep direct cache/frontmatter reads before event, traversal, and
    // collection extraction rules for stable catalog output.
    vec![
        cache_read::rule(),
        frontmatter_read::rule(),
        events::rule(),
        traversal::rule(),
        extract::rule(),
    ]
}
