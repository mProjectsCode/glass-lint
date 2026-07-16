//! Complete Obsidian provider rule catalog.
//!
//! Category modules own their policies; this assembly point fixes category
//! order so metadata, profiles, and findings remain deterministic.

mod codemirror;
mod editor;
mod file_manager;
mod lifecycle;
mod markdown;
mod metadata;
mod network;
mod platform;
mod plugins;
mod storage;
mod ui;
mod vault;
mod view;
mod workspace;

use glass_lint_core::rules::Rule;

pub fn all() -> Vec<Rule> {
    // Keep broad access categories first and lifecycle/platform/plugin rules
    // last; do not rely on module discovery order for catalog stability.
    [
        network::rules(),
        vault::rules(),
        metadata::rules(),
        workspace::rules(),
        view::rules(),
        ui::rules(),
        editor::rules(),
        file_manager::rules(),
        markdown::rules(),
        codemirror::rules(),
        storage::rules(),
        lifecycle::rules(),
        platform::rules(),
        plugins::rules(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
