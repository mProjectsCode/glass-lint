//! Node archive and compression rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
pub fn rule() -> Rule {
    Rule::builder("archive.compression")
        .description("Uses archive or compression libraries")
        .category("node/archive")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::package_import("jszip"))
        .declaration(MatcherDecl::package_import("tar"))
        .declaration(MatcherDecl::package_import("zlib"))
        .declaration(MatcherDecl::import("node:zlib"))
        .declaration(MatcherDecl::package_import("fflate"))
        .declaration(MatcherDecl::package_import("archiver"))
        .declaration(MatcherDecl::package_import("yauzl"))
        .declaration(MatcherDecl::package_import("unzipper"))
        .declaration(MatcherDecl::package_import("node-tar"))
        .declaration(MatcherDecl::package_import("compressing"))
        .declaration(MatcherDecl::package_import("adm-zip"))
        .declaration(MatcherDecl::package_import("extract-zip"))
        .declaration(MatcherDecl::package_import("tar-stream"))
        .declaration(MatcherDecl::package_import("pako"))
        .declaration(MatcherDecl::package_import("decompress"))
        .declaration(MatcherDecl::package_import("zip-a-folder"))
        .declaration(MatcherDecl::package_import("@zip.js/zip.js"))
        .declaration(MatcherDecl::package_import("yazl"))
        .declaration(MatcherDecl::package_import("node-stream-zip"))
        .build()
        .unwrap()
}
