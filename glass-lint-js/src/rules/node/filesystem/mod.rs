//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

const FS_MODULES: &[&str] = &["fs", "fs/promises", "node:fs", "node:fs/promises"];
const FS_METHODS: &[&str] = &[
    "readFile",
    "readFileSync",
    "writeFile",
    "writeFileSync",
    "appendFile",
    "appendFileSync",
    "mkdir",
    "mkdirSync",
    "rm",
    "rmSync",
    "unlink",
    "unlinkSync",
    "rename",
    "renameSync",
    "copyFile",
    "copyFileSync",
    "readdir",
    "readdirSync",
    "stat",
    "statSync",
    "open",
    "openSync",
    "watch",
    "watchFile",
];

/// Detects static ESM or unshadowed CommonJS loads of the exact Node filesystem
/// and configured filesystem packages. Path manipulation is deliberately not
/// included: normalization and joining paths are not filesystem I/O. The
/// rule reports both dependency indicators and proven operations.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    let mut builder = Rule::builder("node.filesystem")
        .description("Uses Node filesystem and path APIs")
        .category(Category::new("node/filesystem").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::import_exact("fs"))
        .query(EventQuery::import_exact("fs/promises"))
        .query(EventQuery::import_exact("node:fs"))
        .query(EventQuery::import_exact("node:fs/promises"))
        .query(EventQuery::import_package("fs-extra"))
        .query(EventQuery::import_package("graceful-fs"))
        .query(EventQuery::import_package("memfs"))
        .query(EventQuery::import_package("unionfs"))
        .query(EventQuery::import_package("chokidar"))
        .query(EventQuery::import_package("proper-lockfile"))
        .query(EventQuery::import_package("tmp"))
        .query(EventQuery::import_package("tmp-promise"))
        .query(EventQuery::import_package("rimraf"))
        .query(EventQuery::import_package("mkdirp"))
        .query(EventQuery::import_package("make-dir"))
        .query(EventQuery::import_package("write-file-atomic"))
        .query(EventQuery::import_package("fs-monkey"))
        .query(EventQuery::import_package("mock-fs"))
        .query(EventQuery::import_package("watchpack"))
        .query(EventQuery::import_package("fsevents"));

    for module in FS_MODULES {
        for method in FS_METHODS {
            builder = builder.query(EventQuery::member_call_module(*module, *method));
            builder = builder.query(EventQuery::call_module(*module, *method));
        }
    }

    builder.build().unwrap()
}
