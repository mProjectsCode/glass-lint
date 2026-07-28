//! Node archive and compression rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("archive.compression")
        .description("Uses archive or compression libraries")
        .category(Category::new("node/archive").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::import_package("jszip"))
        .query(QueryDecl::import_package("tar"))
        .query(QueryDecl::import_package("zlib"))
        .query(QueryDecl::import_exact("node:zlib"))
        .query(QueryDecl::import_package("fflate"))
        .query(QueryDecl::import_package("archiver"))
        .query(QueryDecl::import_package("yauzl"))
        .query(QueryDecl::import_package("unzipper"))
        .query(QueryDecl::import_package("node-tar"))
        .query(QueryDecl::import_package("compressing"))
        .query(QueryDecl::import_package("adm-zip"))
        .query(QueryDecl::import_package("extract-zip"))
        .query(QueryDecl::import_package("tar-stream"))
        .query(QueryDecl::import_package("pako"))
        .query(QueryDecl::import_package("decompress"))
        .query(QueryDecl::import_package("zip-a-folder"))
        .query(QueryDecl::import_package("@zip.js/zip.js"))
        .query(QueryDecl::import_package("yazl"))
        .query(QueryDecl::import_package("node-stream-zip"))
        .build()
        .unwrap()
}
