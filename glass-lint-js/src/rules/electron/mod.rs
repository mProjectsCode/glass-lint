mod dialog;
mod ipc;
mod module;
mod shell;

use glass_lint_core::rules::Rule;

pub(crate) fn rules() -> Vec<Rule> {
    vec![module::rule(), ipc::rule(), shell::rule(), dialog::rule()]
}
