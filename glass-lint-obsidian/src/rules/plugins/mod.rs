mod dataview;
mod other_access;
use glass_lint_core::rules::Rule;
pub(crate) fn rules() -> Vec<Rule> {
    vec![other_access::rule(), dataview::rule()]
}
