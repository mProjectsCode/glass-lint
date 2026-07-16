//! Electron-specific rule catalog.
//!
//! Child modules own module-load and API-use rules; this layer fixes their
//! order for deterministic provider metadata and findings.

mod dialog;
mod ipc;
mod module;
mod shell;

use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    // Report the module boundary before API-specific uses, then keep the API
    // categories in a stable order.
    vec![module::rule(), ipc::rule(), shell::rule(), dialog::rule()]
}
