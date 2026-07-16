//! Node.js provider rule catalog.
//!
//! Child modules distinguish strict module provenance and rooted process reads
//! from intentionally heuristic API-shaped matches.

mod archive_compression;
mod crypto_operation;
mod filesystem;
mod network;
mod process_environment;
mod subprocess;

use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    // Keep network/filesystem/process access ahead of lower-level archive and
    // crypto categories in a fixed order for reproducible catalog output.
    vec![
        network::rule(),
        filesystem::rule(),
        process_environment::rule(),
        subprocess::rule(),
        archive_compression::rule(),
        crypto_operation::rule(),
    ]
}
