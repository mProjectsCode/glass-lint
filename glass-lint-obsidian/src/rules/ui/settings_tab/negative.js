// @case description negative registration and constructor lookalikes
// @tool glass-lint rules=obsidian:ui.settings-tab
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
plugin.addSettingTab(tab);

const addSettingTab = this.addSettingTab;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
addSettingTab(tab);

// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this[dynamicProperty](tab);

// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingsTab(tab);

function shadowed(PluginSettingTab) {
  // @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
  new PluginSettingTab();
}

class LocalSettingTab {}
import { PluginSettingTab as ImportedSettingTab } from "obsidian";
let reassigned = ImportedSettingTab;
reassigned = LocalSettingTab;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
new reassigned();
