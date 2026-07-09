//! Obsidian's disclosure policy, deliberately separate from generic rules.

pub(super) fn for_rule(rule_id: &str) -> &'static [&'static str] {
    match rule_id {
        "network.browser" | "network.node" | "network.remote_dom_loading" => {
            &["disclosure.network_access"]
        }
        "network.obsidian" => &[
            "disclosure.network_access",
            "disclosure.cors_free_network_access",
        ],
        "network.private" => &["disclosure.private_network_access"],
        "network.ai_provider" | "network.sync_storage_provider" => &[
            "disclosure.network_access",
            "disclosure.third_party_services",
        ],
        "network.telemetry" => &[
            "disclosure.network_access",
            "disclosure.telemetry_or_error_reporting",
        ],
        "vault.read" => &["disclosure.note_content_access"],
        "vault.write" | "vault.destructive" => &["disclosure.vault_file_write"],
        "vault.enumerate" => &["disclosure.full_vault_access"],
        "vault.adapter" => &["disclosure.adapter_file_access"],
        "vault.obsidian_config" => &["disclosure.obsidian_config_access"],
        "metadata.read" | "metadata.frontmatter" | "metadata.extraction" => {
            &["disclosure.metadata_access"]
        }
        "metadata.frontmatter_write" => {
            &["disclosure.metadata_access", "disclosure.vault_file_write"]
        }
        "workspace.views" | "workspace.layout_persistence" => &["disclosure.workspace_layout"],
        "editor.extension" | "editor.codemirror" | "editor.suggest" => {
            &["disclosure.editor_behavior"]
        }
        "editor.markdown_processing" => &["disclosure.markdown_processing"],
        "lifecycle.events" | "browser.broad_input_hooks" => {
            &["disclosure.global_handlers_or_timers"]
        }
        "plugins.internal_access" => &["disclosure.plugin_internals"],
        "platform.branching" => &["disclosure.platform_branching"],
        "filesystem.node" => &["disclosure.node_filesystem_access"],
        "process.node" | "electron.ipc_shell" => &["disclosure.process_or_shell_access"],
        "browser.clipboard" => &["disclosure.clipboard_access"],
        "browser.permissions" => &["disclosure.permission_sensitive_browser_api"],
        "browser.environment" => &["disclosure.browser_environment_access"],
        "dynamic_code" => &["disclosure.dynamic_code_or_remote_code"],
        _ => &[],
    }
}
