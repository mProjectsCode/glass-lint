//! Node archive and compression rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("archive.compression")
        .description("Uses archive or compression libraries")
        .category("node/archive")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .import_package("jszip")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("tar")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("zlib")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("node:zlib")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("fflate")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("archiver")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("yauzl")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("unzipper")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("node-tar")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("compressing")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("adm-zip")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("extract-zip")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("tar-stream")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("pako")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("decompress")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("zip-a-folder")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@zip.js/zip.js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("yazl")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("node-stream-zip")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
