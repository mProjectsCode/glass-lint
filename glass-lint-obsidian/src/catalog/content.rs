use super::ObsidianRuleBuilderExt;

use glass_lint_core::rules::{
    Confidence, Matcher, MemberCallMatcher, Rule, Rule as ApiRule, Severity as ApiSeverity,
};

pub(super) fn rules() -> Vec<Rule> {
    vec![
        ApiRule::builder("vault.access")
            .label("Accesses Obsidian vault APIs")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_reads(["app.vault"])
            .build(),
        ApiRule::builder("vault.read")
            .label("Reads vault files")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "app.vault.read",
                "app.vault.cachedRead",
                "app.vault.readBinary",
            ])
            .build(),
        ApiRule::builder("vault.write")
            .label("Writes vault files")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "app.vault.create",
                "app.vault.createBinary",
                "app.vault.modify",
                "app.vault.modifyBinary",
                "app.vault.append",
                "app.vault.appendBinary",
                "app.vault.process",
                "app.vault.createFolder",
            ])
            .build(),
        ApiRule::builder("vault.destructive")
            .label("Renames, deletes, trashes, or copies vault files")
            .category("vault")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "app.vault.delete",
                "app.vault.trash",
                "app.vault.rename",
                "app.vault.copy",
                "app.fileManager.renameFile",
                "app.fileManager.trashFile",
            ])
            .build(),
        ApiRule::builder("vault.enumerate")
            .label("Enumerates vault files")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "app.vault.getFiles",
                "app.vault.getMarkdownFiles",
                "app.vault.getAllLoadedFiles",
                "app.vault.getAllFolders",
            ])
            .build(),
        ApiRule::builder("vault.folder_ops")
            .label("Accesses vault folders")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls(["app.vault.getFolderByPath", "app.vault.getRoot"])
            .build(),
        ApiRule::builder("vault.resources")
            .label("Accesses attachment resource paths")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "app.vault.getResourcePath",
                "app.vault.adapter.getResourcePath",
            ])
            .build(),
        ApiRule::builder("vault.adapter")
            .label("Uses adapter-level vault filesystem APIs")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_reads(["app.vault.adapter"])
            .build(),
        ApiRule::builder("vault.obsidian_config")
            .label("References .obsidian configuration paths")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_string_literals([".obsidian/", ".obsidian\\"])
            .build(),
        ApiRule::builder("vault.uri")
            .label("References Obsidian URI links")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_string_literals(["obsidian://"])
            .build(),
        ApiRule::builder("vault.open_create_flows")
            .label("Opens or creates files through workspace or file manager APIs")
            .category("vault")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls(["app.workspace.openLinkText"])
            .with_rooted_member_calls(["app.workspace.getLeaf.openFile"])
            .with_heuristic_member_calls(["leaf.openFile"])
            .build(),
        ApiRule::builder("metadata.read")
            .label("Reads Obsidian metadata cache")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_reads([
                "app.metadataCache",
                "app.metadataCache.resolvedLinks",
                "app.metadataCache.unresolvedLinks",
            ])
            .with_rooted_member_calls([
                "app.metadataCache.getFileCache",
                "app.metadataCache.getCache",
                "app.metadataCache.getFirstLinkpathDest",
            ])
            .build(),
        ApiRule::builder("metadata.frontmatter")
            .label("Reads cached frontmatter")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_rooted_member_reads(["app.metadataCache.getFileCache.frontmatter"])
            .build(),
        ApiRule::builder("metadata.frontmatter_write")
            .label("Updates frontmatter through Obsidian APIs")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls(["app.fileManager.processFrontMatter"])
            .build(),
        ApiRule::builder("metadata.events")
            .label("Registers metadata cache event listeners")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("app.metadataCache.on")
                    .arg_string(0, ["changed", "deleted", "resolved"]),
            ))
            .build(),
        ApiRule::builder("metadata.traversal")
            .label("Traverses metadata cache maps or cached metadata for many files")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("Object.entries").arg_rooted_exprs(
                    0,
                    [
                        "app.metadataCache.resolvedLinks",
                        "app.metadataCache.unresolvedLinks",
                    ],
                ),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("Object.keys").arg_rooted_exprs(
                    0,
                    [
                        "app.metadataCache.resolvedLinks",
                        "app.metadataCache.unresolvedLinks",
                    ],
                ),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("Object.values").arg_rooted_exprs(
                    0,
                    [
                        "app.metadataCache.resolvedLinks",
                        "app.metadataCache.unresolvedLinks",
                    ],
                ),
            ))
            .build(),
        ApiRule::builder("metadata.extraction")
            .label("Extracts tags, links, embeds, blocks, or headings from metadata")
            .category("metadata")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_rooted_member_reads([
                "app.metadataCache.getFileCache.tags",
                "app.metadataCache.getFileCache.links",
                "app.metadataCache.getFileCache.embeds",
                "app.metadataCache.getFileCache.blocks",
                "app.metadataCache.getFileCache.headings",
                "app.metadataCache.getFileCache.sections",
            ])
            .build(),
        ApiRule::builder("dependency.dataview")
            .label("References Dataview or DataCore plugin APIs")
            .category("dependency")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_string_literals(["dataview", "dataviewapi", "data-core", "datacore"])
            .build(),
    ]
    .into_iter()
    .map(|rule| rule.expect("built-in Obsidian rule should be valid"))
    .collect()
}
