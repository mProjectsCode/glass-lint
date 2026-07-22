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
        .matcher(Matcher::package_import("fs-extra"))
        .matcher(Matcher::package_import("graceful-fs"))
        .matcher(Matcher::package_import("memfs"))
        .matcher(Matcher::package_import("unionfs"))
        .matcher(Matcher::package_import("chokidar"))
        .matcher(Matcher::package_import("proper-lockfile"))
        .matcher(Matcher::package_import("tmp"))
        .matcher(Matcher::package_import("tmp-promise"))
        .matcher(Matcher::package_import("rimraf"))
        .matcher(Matcher::package_import("mkdirp"))
        .matcher(Matcher::package_import("make-dir"))
        .matcher(Matcher::package_import("write-file-atomic"))
        .matcher(Matcher::package_import("fs-monkey"))
        .matcher(Matcher::package_import("mock-fs"))
        .matcher(Matcher::package_import("watchpack"))
        .matcher(Matcher::package_import("fsevents"));

    for module in PATH_MODULES {
        for method in PATH_METHODS {
            builder = builder.matcher(Matcher::module_member_call(*module, *method));
            builder = builder.matcher(Matcher::module_call(*module, *method));
        }
    }

    builder.build().unwrap()
}
