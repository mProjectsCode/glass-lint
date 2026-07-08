use glass_lint_core::rules::{Confidence, Rule, Rule as ApiRule, Severity as ApiSeverity};

pub(super) fn rules() -> Vec<Rule> {
    vec![
        ApiRule::builder("workspace.access")
            .label("Accesses Obsidian workspace APIs")
            .category("workspace")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .rooted_member_reads(["app.workspace"])
            .build(),
        ApiRule::builder("workspace.views")
            .label("Registers or manipulates workspace views and panes")
            .category("workspace")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .rooted_member_calls([
                "this.registerView",
                "app.workspace.getLeavesOfType",
                "app.workspace.detachLeavesOfType",
                "app.workspace.revealLeaf",
                "app.workspace.getLeaf.openFile",
            ])
            .implies(["disclosure.workspace_layout"])
            .build(),
        ApiRule::builder("workspace.active_file")
            .label("Accesses the active file or editor")
            .category("workspace")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .rooted_member_reads(["app.workspace.activeEditor"])
            .rooted_member_calls(["app.workspace.getActiveFile"])
            .build(),
        ApiRule::builder("workspace.editor_commands")
            .label("Registers editor callbacks, menus, or command palette integrations")
            .category("workspace")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_call("this.addCommand")
            .arg_object_keys(0, ["editorCallback"])
            .member_call("app.workspace.on")
            .arg_string(0, ["file-menu", "editor-menu"])
            .build(),
        ApiRule::builder("workspace.layout_persistence")
            .label("Reads or writes workspace layout persistence")
            .category("workspace")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .rooted_member_calls([
                "app.workspace.getLayout",
                "app.workspace.changeLayout",
                "app.workspace.requestSaveLayout",
            ])
            .implies(["disclosure.workspace_layout"])
            .build(),
        ApiRule::builder("ui.commands")
            .label("Registers commands, ribbon icons, or status bar items")
            .category("ui")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls([
                "this.addCommand",
                "this.addRibbonIcon",
                "this.addStatusBarItem",
            ])
            .build(),
        ApiRule::builder("ui.modals_notices")
            .label("Uses Obsidian modal or notice UI")
            .category("ui")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .constructors([
                "Modal",
                "Notice",
                "SuggestModal",
                "FuzzySuggestModal",
                "obsidian.Modal",
                "obsidian.Notice",
                "obsidian.SuggestModal",
                "obsidian.FuzzySuggestModal",
            ])
            .classes([
                "obsidian.Modal",
                "obsidian.Notice",
                "obsidian.SuggestModal",
                "obsidian.FuzzySuggestModal",
            ])
            .build(),
        ApiRule::builder("ui.dom_heavy")
            .label("Uses low-level DOM APIs")
            .category("ui")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_calls([
                "document.createElement",
                "document.querySelector",
                "document.querySelectorAll",
                "document.body.appendChild",
                "document.head.appendChild",
                "document.documentElement.appendChild",
            ])
            .calls(["createEl"])
            .constructors(["MutationObserver"])
            .build(),
        ApiRule::builder("ui.file_dialog")
            .label("Uses file dialogs or DOM file inputs")
            .category("ui")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_calls(["dialog.showOpenDialog", "dialog.showSaveDialog"])
            .member_call("document.createElement")
            .arg_string(0, ["input"])
            .assigned_property("type", ["file"])
            .build(),
        ApiRule::builder("editor.extension")
            .label("Registers editor extensions")
            .category("editor")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls(["this.registerEditorExtension"])
            .implies(["disclosure.editor_behavior"])
            .build(),
        ApiRule::builder("editor.markdown_processing")
            .label("Registers markdown processors or renderers")
            .category("editor")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls([
                "this.registerMarkdownPostProcessor",
                "this.registerMarkdownCodeBlockProcessor",
                "MarkdownRenderer.render",
                "obsidian.MarkdownRenderer.render",
            ])
            .implies(["disclosure.markdown_processing"])
            .build(),
        ApiRule::builder("editor.markdown_api")
            .label("Uses markdown view, editor, or link helper APIs")
            .category("editor")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_calls([
                "editor.getValue",
                "editor.replaceRange",
                "editor.transaction",
            ])
            .classes(["obsidian.MarkdownView", "obsidian.Editor"])
            .module_calls(
                "obsidian",
                ["parseLinktext", "normalizePath", "getLinkpath"],
            )
            .module_member_calls(
                "obsidian",
                ["parseLinktext", "normalizePath", "getLinkpath"],
            )
            .build(),
        ApiRule::builder("editor.codemirror")
            .label("References CodeMirror extension primitives")
            .category("editor")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .imports([
                "@codemirror/state",
                "@codemirror/view",
                "@codemirror/language",
                "@codemirror/commands",
            ])
            .implies(["disclosure.editor_behavior"])
            .build(),
        ApiRule::builder("editor.suggest")
            .label("Registers editor suggestions")
            .category("editor")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_calls(["this.registerEditorSuggest"])
            .classes(["obsidian.EditorSuggest"])
            .implies(["disclosure.editor_behavior"])
            .build(),
        ApiRule::builder("settings.persistence")
            .label("Persists plugin settings or data")
            .category("settings")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls(["this.loadData", "this.saveData"])
            .build(),
        ApiRule::builder("settings.ui")
            .label("Registers plugin settings UI")
            .category("settings")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls(["this.addSettingTab"])
            .constructors([
                "PluginSettingTab",
                "Setting",
                "obsidian.PluginSettingTab",
                "obsidian.Setting",
            ])
            .classes(["obsidian.PluginSettingTab", "obsidian.Setting"])
            .member_calls(["this.loadData", "this.saveData"])
            .build(),
        ApiRule::builder("lifecycle.methods")
            .label("Defines plugin lifecycle methods")
            .category("lifecycle")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .member_reads(["onload", "onunload"])
            .build(),
        ApiRule::builder("lifecycle.events")
            .label("Registers events, DOM handlers, or intervals")
            .category("lifecycle")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .member_calls([
                "this.registerEvent",
                "this.registerDomEvent",
                "this.registerInterval",
            ])
            .global_calls(["setInterval", "setTimeout", "requestAnimationFrame"])
            .implies(["disclosure.global_handlers_or_timers"])
            .build(),
    ]
    .into_iter()
    .map(|rule| rule.expect("built-in Obsidian rule should be valid"))
    .collect()
}
