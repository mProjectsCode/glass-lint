//! Node archive and compression rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
pub fn rule() -> Rule {
    Rule::builder("archive.compression")
        .description("Uses archive or compression libraries")
        .category("node/archive")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::package_import("jszip").unwrap())
        .matcher(Matcher::package_import("tar").unwrap())
        .matcher(Matcher::package_import("zlib").unwrap())
        .matcher(Matcher::import("node:zlib"))
        .matcher(Matcher::package_import("fflate").unwrap())
        .matcher(Matcher::package_import("archiver").unwrap())
        .matcher(Matcher::package_import("yauzl").unwrap())
        .matcher(Matcher::package_import("unzipper").unwrap())
        .matcher(Matcher::package_import("node-tar").unwrap())
        .matcher(Matcher::package_import("compressing").unwrap())
        .matcher(Matcher::package_import("adm-zip").unwrap())
        .matcher(Matcher::package_import("extract-zip").unwrap())
        .matcher(Matcher::package_import("tar-stream").unwrap())
        .matcher(Matcher::package_import("pako").unwrap())
        .matcher(Matcher::package_import("decompress").unwrap())
        .matcher(Matcher::package_import("zip-a-folder").unwrap())
        .matcher(Matcher::package_import("@zip.js/zip.js").unwrap())
        .matcher(Matcher::package_import("yazl").unwrap())
        .matcher(Matcher::package_import("node-stream-zip").unwrap())
        .build()
        .unwrap()
}
