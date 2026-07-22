//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::import("fs"))
        .declaration(MatcherDecl::import("fs/promises"))
        .declaration(MatcherDecl::import("node:fs"))
        .declaration(MatcherDecl::import("node:fs/promises"))
        .declaration(MatcherDecl::package_import("fs-extra"))
        .declaration(MatcherDecl::package_import("graceful-fs"))
        .declaration(MatcherDecl::package_import("memfs"))
        .declaration(MatcherDecl::package_import("unionfs"))
        .declaration(MatcherDecl::package_import("chokidar"))
        .declaration(MatcherDecl::package_import("proper-lockfile"))
        .declaration(MatcherDecl::package_import("tmp"))
        .declaration(MatcherDecl::package_import("tmp-promise"))
        .declaration(MatcherDecl::package_import("rimraf"))
        .declaration(MatcherDecl::package_import("mkdirp"))
        .declaration(MatcherDecl::package_import("make-dir"))
        .declaration(MatcherDecl::package_import("write-file-atomic"))
        .declaration(MatcherDecl::package_import("fs-monkey"))
        .declaration(MatcherDecl::package_import("mock-fs"))
        .declaration(MatcherDecl::package_import("watchpack"))
        .declaration(MatcherDecl::package_import("fsevents"));

    for module in PATH_MODULES {
        for method in PATH_METHODS {
            builder = builder.declaration(MatcherDecl::module_member_call(*module, *method));
            builder = builder.declaration(MatcherDecl::module_call(*module, *method));
        }
    }

    builder.build().unwrap()
}
