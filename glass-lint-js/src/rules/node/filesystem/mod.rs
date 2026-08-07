//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

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
const FS_PACKAGES: &[&str] = &[
    "fs-extra",
    "graceful-fs",
    "memfs",
    "unionfs",
    "chokidar",
    "proper-lockfile",
    "tmp",
    "tmp-promise",
    "rimraf",
    "mkdirp",
    "make-dir",
    "write-file-atomic",
    "fs-monkey",
    "mock-fs",
    "watchpack",
    "fsevents",
];

/// Detects static ESM or unshadowed CommonJS loads of the exact Node filesystem
/// and configured filesystem packages. Path manipulation is deliberately not
/// included: normalization and joining paths are not filesystem I/O. The
/// rule reports both dependency indicators and proven operations.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("node.filesystem")
        .description("Uses Node filesystem and path APIs")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .queries(FS_MODULES.iter().copied().map(EventQuery::import_exact))
        .queries(FS_PACKAGES.iter().copied().map(EventQuery::import_package));

    for module in FS_MODULES {
        for method in FS_METHODS {
            builder = builder.query(EventQuery::member_call_module(*module, *method));
            builder = builder.query(EventQuery::call_module(*module, *method));
        }
    }

    builder.build().unwrap()
}
