mod access;
mod enable_disable;
mod load_unload;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![access::rule(), enable_disable::rule(), load_unload::rule()]
}
