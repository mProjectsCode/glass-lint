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
        .matcher(Matcher::package_import("jszip"))
        .matcher(Matcher::package_import("tar"))
        .matcher(Matcher::package_import("zlib"))
        .matcher(Matcher::import("node:zlib"))
        .matcher(Matcher::package_import("fflate"))
        .matcher(Matcher::package_import("archiver"))
        .matcher(Matcher::package_import("yauzl"))
        .matcher(Matcher::package_import("unzipper"))
        .matcher(Matcher::package_import("node-tar"))
        .matcher(Matcher::package_import("compressing"))
        .matcher(Matcher::package_import("adm-zip"))
        .matcher(Matcher::package_import("extract-zip"))
        .matcher(Matcher::package_import("tar-stream"))
        .matcher(Matcher::package_import("pako"))
        .matcher(Matcher::package_import("decompress"))
        .matcher(Matcher::package_import("zip-a-folder"))
        .matcher(Matcher::package_import("@zip.js/zip.js"))
        .matcher(Matcher::package_import("yazl"))
        .matcher(Matcher::package_import("node-stream-zip"))
        .build()
        .unwrap()
}
