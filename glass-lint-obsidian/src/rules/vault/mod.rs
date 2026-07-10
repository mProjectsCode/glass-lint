mod access;
mod adapter;
mod config_directory;
mod delete;
mod enumerate;
mod events;
mod move_copy;
mod read;
mod resource_url;
mod write;
use glass_lint_core::rules::Rule;
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        access::rule(),
        read::rule(),
        write::rule(),
        delete::rule(),
        move_copy::rule(),
        enumerate::rule(),
        adapter::rule(),
        config_directory::rule(),
        resource_url::rule(),
        events::rule(),
    ]
}
