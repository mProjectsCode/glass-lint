//! CodeMirror extension module rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects static ESM imports and unshadowed CommonJS loads of the exact
/// CodeMirror packages used by the provider. The finding is attached to the
/// module load, not later API use; similar package names, dynamic module names,
/// and shadowed `require` loaders are excluded by module provenance.
pub fn rule() -> Rule {
    Rule::builder("codemirror.extension")
        .description("Uses CodeMirror extension primitives")
        .category("codemirror")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::package_import("@codemirror/state"))
        .declaration(MatcherDecl::package_import("@codemirror/view"))
        .declaration(MatcherDecl::package_import("@codemirror/language"))
        .declaration(MatcherDecl::package_import("@codemirror/commands"))
        .declaration(MatcherDecl::package_import("@codemirror/lang-markdown"))
        .declaration(MatcherDecl::package_import("@codemirror/lang-javascript"))
        .declaration(MatcherDecl::package_import("@codemirror/lang-json"))
        .declaration(MatcherDecl::package_import("@codemirror/autocomplete"))
        .declaration(MatcherDecl::package_import("@codemirror/lint"))
        .declaration(MatcherDecl::package_import("@codemirror/search"))
        .declaration(MatcherDecl::package_import("@codemirror/collab"))
        .declaration(MatcherDecl::package_import("@lezer/common"))
        .declaration(MatcherDecl::package_import("@lezer/highlight"))
        .declaration(MatcherDecl::package_import("@lezer/lr"))
        .declaration(MatcherDecl::package_import("@lezer/javascript"))
        .declaration(MatcherDecl::package_import("@lezer/markdown"))
        .build()
        .unwrap()
}
