//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    let mut builder = Rule::builder("node.filesystem")
        .description("Uses Node filesystem and path APIs")
        .category(Category::new("node/filesystem").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::import_exact("fs"))
        .query(QueryDecl::import_exact("fs/promises"))
        .query(QueryDecl::import_exact("node:fs"))
        .query(QueryDecl::import_exact("node:fs/promises"))
        .query(QueryDecl::import_package("fs-extra"))
        .query(QueryDecl::import_package("graceful-fs"))
        .query(QueryDecl::import_package("memfs"))
        .query(QueryDecl::import_package("unionfs"))
        .query(QueryDecl::import_package("chokidar"))
        .query(QueryDecl::import_package("proper-lockfile"))
        .query(QueryDecl::import_package("tmp"))
        .query(QueryDecl::import_package("tmp-promise"))
        .query(QueryDecl::import_package("rimraf"))
        .query(QueryDecl::import_package("mkdirp"))
        .query(QueryDecl::import_package("make-dir"))
        .query(QueryDecl::import_package("write-file-atomic"))
        .query(QueryDecl::import_package("fs-monkey"))
        .query(QueryDecl::import_package("mock-fs"))
        .query(QueryDecl::import_package("watchpack"))
        .query(QueryDecl::import_package("fsevents"));

    for module in PATH_MODULES {
        for method in PATH_METHODS {
            builder = builder.query(QueryDecl::member_call_module(*module, *method));
            builder = builder.query(QueryDecl::call_module(*module, *method));
        }
    }

    builder.build().unwrap()
}
