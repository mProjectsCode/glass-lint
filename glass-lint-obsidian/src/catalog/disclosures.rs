//! Obsidian-specific disclosure policy.

pub(super) fn for_rule(id: &str) -> &'static [&'static str] {
    match id {
        "network.request" => &[
            "disclosure.network_access",
            "disclosure.cors_free_network_access",
        ],
        "vault.read" | "metadata.cache-read" | "metadata.frontmatter-read" | "metadata.extract" => {
            &["disclosure.note_content_access"]
        }
        "vault.write" | "vault.delete" | "vault.move-copy" | "metadata.frontmatter-write" => {
            &["disclosure.vault_file_write"]
        }
        "vault.enumerate" => &["disclosure.full_vault_access"],
        "vault.adapter" => &["disclosure.adapter_file_access"],
        "vault.config-directory" => &["disclosure.obsidian_config_access"],
        "metadata.events" | "lifecycle.events" => &["disclosure.global_handlers_or_timers"],
        "workspace.layout" | "workspace.leaf-management" => &["disclosure.workspace_layout"],
        "editor.extension" | "editor.suggest" | "codemirror.extension" => {
            &["disclosure.editor_behavior"]
        }
        "markdown.postprocessor"
        | "markdown.code-block-processor"
        | "markdown.render"
        | "markdown.link" => &["disclosure.markdown_processing"],
        "plugins.other-access" => &["disclosure.plugin_internals"],
        "platform.branching" => &["disclosure.platform_branching"],
        _ => &[],
    }
}
