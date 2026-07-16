mod branching;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![branching::rule()]
}
