mod codemirror;
mod editor;
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

pub(crate) fn all() -> Vec<Rule> {
    [
        network::rules(),
        vault::rules(),
        metadata::rules(),
        workspace::rules(),
        view::rules(),
        ui::rules(),
        editor::rules(),
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
