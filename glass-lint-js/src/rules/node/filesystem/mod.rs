//! Node filesystem and path module rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .import_exact("fs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("fs/promises")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("node:fs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("node:fs/promises")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("fs-extra")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("graceful-fs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("memfs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("unionfs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("chokidar")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("proper-lockfile")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("tmp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("tmp-promise")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("rimraf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("mkdirp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("make-dir")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("write-file-atomic")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("fs-monkey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("mock-fs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("watchpack")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("fsevents")
                .build()
                .expect("valid matcher declaration"),
        );

    for module in PATH_MODULES {
        for method in PATH_METHODS {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .member_call_module(*module, *method)
                    .build()
                    .expect("valid matcher declaration"),
            );
            builder = builder.declaration(
                MatcherDecl::builder()
                    .call_module(*module, *method)
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }

    builder.build().unwrap()
}
