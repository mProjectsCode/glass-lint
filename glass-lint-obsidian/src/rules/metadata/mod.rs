mod cache_read;
mod events;
mod extract;
mod frontmatter_read;
mod traversal;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![
        cache_read::rule(),
        frontmatter_read::rule(),
        events::rule(),
        traversal::rule(),
        extract::rule(),
    ]
}
