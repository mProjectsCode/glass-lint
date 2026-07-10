use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects imports and unshadowed static CommonJS/interop loads of the exact
/// `electron` module. The report is attached to the module load itself and
/// does not require a later API call; similarly named modules and shadowed
/// `require` calls are excluded.
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
