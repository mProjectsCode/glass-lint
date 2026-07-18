//! Obsidian user-interface rule catalog.
//!
//! The catalog combines strict plugin-instance and module provenance rules
//! with explicitly heuristic menu behavior.

mod command;
mod menu;
mod modal;
mod notice;
mod ribbon;
mod settings_tab;
mod status_bar;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    // Registration, status, modal, and notice rules precede the broader menu
    // and settings heuristics in a stable order.
    vec![
        command::rule(),
        ribbon::rule(),
        status_bar::rule(),
        modal::rule(),
        notice::rule(),
        menu::rule(),
        settings_tab::rule(),
    ]
}
