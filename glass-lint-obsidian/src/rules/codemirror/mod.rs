mod extension;
use glass_lint_core::rules::Rule;
pub(crate) fn rules() -> Vec<Rule> {
    vec![extension::rule()]
}
