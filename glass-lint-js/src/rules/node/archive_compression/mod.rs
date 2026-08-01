//! Node archive and compression rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, QueryBuildError, Rule, Severity};

#[derive(Clone, Copy)]
enum ImportSpec {
    Package(&'static str),
    Exact(&'static str),
}

const IMPORTS: &[ImportSpec] = &[
    ImportSpec::Package("jszip"),
    ImportSpec::Package("tar"),
    ImportSpec::Package("zlib"),
    ImportSpec::Exact("node:zlib"),
    ImportSpec::Package("fflate"),
    ImportSpec::Package("archiver"),
    ImportSpec::Package("yauzl"),
    ImportSpec::Package("unzipper"),
    ImportSpec::Package("node-tar"),
    ImportSpec::Package("compressing"),
    ImportSpec::Package("adm-zip"),
    ImportSpec::Package("extract-zip"),
    ImportSpec::Package("tar-stream"),
    ImportSpec::Package("pako"),
    ImportSpec::Package("decompress"),
    ImportSpec::Package("zip-a-folder"),
    ImportSpec::Package("@zip.js/zip.js"),
    ImportSpec::Package("yazl"),
    ImportSpec::Package("node-stream-zip"),
];

fn import_query(spec: ImportSpec) -> Result<EventQuery, QueryBuildError> {
    match spec {
        ImportSpec::Package(module) => EventQuery::import_package(module),
        ImportSpec::Exact(module) => EventQuery::import_exact(module),
    }
}

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
pub fn rule() -> Rule {
    Rule::builder("archive.compression")
        .description("Uses archive or compression libraries")
        .category(Category::new("node/archive").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .queries(IMPORTS.iter().copied().map(import_query))
        .build()
        .unwrap()
}
