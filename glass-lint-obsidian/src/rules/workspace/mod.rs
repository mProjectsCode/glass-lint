mod active_editor;
mod active_file;
mod layout;
mod leaf_management;
mod open;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![
        active_file::rule(),
        active_editor::rule(),
        open::rule(),
        leaf_management::rule(),
        layout::rule(),
    ]
}
