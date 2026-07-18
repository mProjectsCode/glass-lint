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
        .matcher(Matcher::import("jszip"))
        .matcher(Matcher::import("tar"))
        .matcher(Matcher::import("zlib"))
        .matcher(Matcher::import("node:zlib"))
        .matcher(Matcher::import("fflate"))
        .matcher(Matcher::import("archiver"))
        .matcher(Matcher::import("yauzl"))
        .matcher(Matcher::import("unzipper"))
        .matcher(Matcher::import("node-tar"))
        .matcher(Matcher::import("compressing"))
        .matcher(Matcher::import("adm-zip"))
        .matcher(Matcher::import("extract-zip"))
        .matcher(Matcher::import("tar-stream"))
        .matcher(Matcher::import("pako"))
        .matcher(Matcher::import("decompress"))
        .matcher(Matcher::import("zip-a-folder"))
        .matcher(Matcher::import("@zip.js/zip.js"))
        .matcher(Matcher::import("yazl"))
        .matcher(Matcher::import("node-stream-zip"))
        .build()
        .unwrap()
}
