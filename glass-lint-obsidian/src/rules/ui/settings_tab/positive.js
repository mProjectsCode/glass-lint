// @case description positive fixture for obsidian:ui.settings-tab
// @tool glass-lint rules=obsidian:ui.settings-tab
// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingTab(tab);
// second independent example

// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingTab(secondTab);
// Migrated: interface/classes-and-settings.js
import { PluginSettingTab as pluginSettingTab } from "obsidian";
class settings extends pluginSettingTab { // @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
  getSettingDefinitions() { return []; }
}
