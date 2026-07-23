// @case description negative registration and constructor lookalikes
// @tool glass-lint rules=obsidian:ui.settings-tab
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
plugin.addSettingTab(tab);

const addSettingTab = this.addSettingTab;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
addSettingTab(tab);

// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
this[dynamicProperty](tab);

// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
this.addSettingsTab(tab);

function shadowed(PluginSettingTab) {
  // @expect-no-error glass-lint rule=obsidian:ui.settings-tab
  new PluginSettingTab();
}

class LocalSettingTab {}
import { PluginSettingTab as ImportedSettingTab } from "obsidian";
let reassigned = ImportedSettingTab;
reassigned = LocalSettingTab;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
new reassigned();
