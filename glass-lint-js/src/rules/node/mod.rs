mod archive_compression;
mod crypto_operation;
mod filesystem;
mod network;
mod process_environment;
mod subprocess;

use glass_lint_core::rules::Rule;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        network::rule(),
        filesystem::rule(),
        process_environment::rule(),
        subprocess::rule(),
        archive_compression::rule(),
        crypto_operation::rule(),
    ]
}
