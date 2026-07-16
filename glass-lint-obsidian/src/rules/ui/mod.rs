mod command;
mod menu;
mod modal;
mod notice;
mod ribbon;
mod settings_tab;
mod status_bar;
mod suggest;
use glass_lint_core::rules::Rule;
pub fn rules() -> Vec<Rule> {
    vec![
        command::rule(),
        ribbon::rule(),
        status_bar::rule(),
        modal::rule(),
        notice::rule(),
        menu::rule(),
        settings_tab::rule(),
        suggest::rule(),
    ]
}
