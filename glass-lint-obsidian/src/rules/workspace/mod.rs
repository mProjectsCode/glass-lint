//! Obsidian workspace rule catalog.
//!
//! Workspace rules share rooted `app.workspace` provenance while separating
//! active-file/editor access, opening, leaf management, and layout operations.

mod active_editor;
mod active_file;
mod layout;
mod leaf_management;
mod open;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    // Keep direct active-context reads before opening, leaf, and layout
    // operations for a stable catalog order.
    vec![
        active_file::rule(),
        active_editor::rule(),
        open::rule(),
        leaf_management::rule(),
        layout::rule(),
    ]
}
