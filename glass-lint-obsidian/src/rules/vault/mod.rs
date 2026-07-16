//! Obsidian vault access and mutation rule catalog.
//!
//! Vault rules share rooted `app.vault` provenance while keeping read, write,
//! enumeration, movement, adapter, and literal-indicator policies separate.

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
pub fn rules() -> Vec<Rule> {
    // Put direct access and file mutation before adapter/path/event indicators
    // in a fixed order for deterministic provider catalogs.
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
