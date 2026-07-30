//! Node archive and compression rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

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
        .query(EventQuery::import_package("jszip"))
        .query(EventQuery::import_package("tar"))
        .query(EventQuery::import_package("zlib"))
        .query(EventQuery::import_exact("node:zlib"))
        .query(EventQuery::import_package("fflate"))
        .query(EventQuery::import_package("archiver"))
        .query(EventQuery::import_package("yauzl"))
        .query(EventQuery::import_package("unzipper"))
        .query(EventQuery::import_package("node-tar"))
        .query(EventQuery::import_package("compressing"))
        .query(EventQuery::import_package("adm-zip"))
        .query(EventQuery::import_package("extract-zip"))
        .query(EventQuery::import_package("tar-stream"))
        .query(EventQuery::import_package("pako"))
        .query(EventQuery::import_package("decompress"))
        .query(EventQuery::import_package("zip-a-folder"))
        .query(EventQuery::import_package("@zip.js/zip.js"))
        .query(EventQuery::import_package("yazl"))
        .query(EventQuery::import_package("node-stream-zip"))
        .build()
        .unwrap()
}
