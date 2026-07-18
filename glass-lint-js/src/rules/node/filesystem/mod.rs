//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

const PATH_MODULES: &[&str] = &["path", "node:path"];
const PATH_METHODS: &[&str] = &[
    "normalize",
    "join",
    "resolve",
    "isAbsolute",
    "relative",
    "toNamespacedPath",
    "dirname",
    "basename",
    "extname",
    "format",
    "parse",
];

/// Detects static ESM or unshadowed CommonJS loads of the exact Node filesystem
/// and path module names and configured filesystem packages. The finding is
/// attached to the module load and does not infer later API use, local names,
/// or similarly named packages; shadowed loaders and non-listed modules are
/// excluded.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("node.filesystem")
        .description("Uses Node filesystem and path APIs")
        .category("node/filesystem")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("fs"))
        .matcher(Matcher::import("fs/promises"))
        .matcher(Matcher::import("node:fs"))
        .matcher(Matcher::import("node:fs/promises"))
        .matcher(Matcher::import("fs-extra"))
        .matcher(Matcher::import("graceful-fs"))
        .matcher(Matcher::import("memfs"))
        .matcher(Matcher::import("unionfs"))
        .matcher(Matcher::import("chokidar"))
        .matcher(Matcher::import("proper-lockfile"))
        .matcher(Matcher::import("tmp"))
        .matcher(Matcher::import("tmp-promise"))
        .matcher(Matcher::import("rimraf"))
        .matcher(Matcher::import("mkdirp"))
        .matcher(Matcher::import("make-dir"))
        .matcher(Matcher::import("write-file-atomic"))
        .matcher(Matcher::import("fs-monkey"))
        .matcher(Matcher::import("mock-fs"))
        .matcher(Matcher::import("watchpack"))
        .matcher(Matcher::import("fsevents"));

    for module in PATH_MODULES {
        for method in PATH_METHODS {
            builder = builder.matcher(Matcher::module_member_call(*module, *method));
            builder = builder.matcher(Matcher::module_call(*module, *method));
        }
    }

    builder.build().unwrap()
}
