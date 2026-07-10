use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects direct ESM or unshadowed CommonJS imports of the listed archive and
/// compression packages. This rule reports the module load itself; it does not
/// infer use from local API names or from similarly named packages.
pub(crate) fn rule() -> Rule {
    Rule::builder("archive.compression")
        .label("Uses archive or compression libraries")
        .category("node/archive")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("jszip"))
        .matcher(Matcher::import("tar"))
        .matcher(Matcher::import("zlib"))
        .matcher(Matcher::import("node:zlib"))
        .matcher(Matcher::import("fflate"))
        .build()
        .unwrap()
}
