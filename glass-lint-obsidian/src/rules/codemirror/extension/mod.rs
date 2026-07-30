//! CodeMirror extension module rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM imports and unshadowed CommonJS loads of the exact
/// CodeMirror packages used by the provider. The finding is attached to the
/// module load, not later API use; similar package names, dynamic module names,
/// and shadowed `require` loaders are excluded by module provenance.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("codemirror.extension")
        .description("Uses CodeMirror extension primitives")
        .category(Category::new("codemirror").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::import_package("@codemirror/state"))
        .query(EventQuery::import_package("@codemirror/view"))
        .query(EventQuery::import_package("@codemirror/language"))
        .query(EventQuery::import_package("@codemirror/commands"))
        .query(EventQuery::import_package("@codemirror/lang-markdown"))
        .query(EventQuery::import_package("@codemirror/lang-javascript"))
        .query(EventQuery::import_package("@codemirror/lang-json"))
        .query(EventQuery::import_package("@codemirror/autocomplete"))
        .query(EventQuery::import_package("@codemirror/lint"))
        .query(EventQuery::import_package("@codemirror/search"))
        .query(EventQuery::import_package("@codemirror/collab"))
        .query(EventQuery::import_package("@lezer/common"))
        .query(EventQuery::import_package("@lezer/highlight"))
        .query(EventQuery::import_package("@lezer/lr"))
        .query(EventQuery::import_package("@lezer/javascript"))
        .query(EventQuery::import_package("@lezer/markdown"))
        .build()
        .unwrap()
}
