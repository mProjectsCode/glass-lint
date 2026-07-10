use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("electron.module")
        .label("Uses Electron APIs")
        .category("electron/module")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("electron"))
        .build()
        .unwrap()
}
