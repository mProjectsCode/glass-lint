mod plugin_data_read;
mod plugin_data_write;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![plugin_data_read::rule(), plugin_data_write::rule()]
}
