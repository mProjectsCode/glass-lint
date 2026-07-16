mod frontmatter_write;

use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    vec![frontmatter_write::rule()]
}
