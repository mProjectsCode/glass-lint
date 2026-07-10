mod cache_read;
mod events;
mod extract;
mod frontmatter_read;
mod frontmatter_write;
mod traversal;
use glass_lint_core::rules::Rule;
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        cache_read::rule(),
        frontmatter_read::rule(),
        frontmatter_write::rule(),
        events::rule(),
        traversal::rule(),
        extract::rule(),
    ]
}
