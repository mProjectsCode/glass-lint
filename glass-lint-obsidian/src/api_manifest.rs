//! Versioned public Obsidian API contract used for catalog drift checks.
//!
//! The public type definitions are the source of truth for these entries.
//! Plugin-manager rules are enabled by default, but their runtime source is
//! tracked separately because those names are not in the public definitions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObsidianApiManifest {
    pub version: &'static str,
    pub vault_events: &'static [&'static str],
    pub metadata_events: &'static [&'static str],
    pub top_level_helpers: &'static [&'static str],
}

pub const PUBLIC_API: ObsidianApiManifest = ObsidianApiManifest {
    version: "obsidian-api@2026-07-31",
    vault_events: &["create", "delete", "modify", "rename"],
    metadata_events: &["changed", "deleted", "resolve", "resolved"],
    top_level_helpers: &[
        "parseLinktext",
        "normalizePath",
        "getLinkpath",
        "resolveSubpath",
        "parseFrontMatterAliases",
        "parseFrontMatterTags",
        "parseFrontMatterEntry",
        "parseFrontMatterStringArray",
    ],
};
