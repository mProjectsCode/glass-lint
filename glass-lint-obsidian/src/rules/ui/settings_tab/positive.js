// @case description registration and module-provenance settings tabs
// @tool glass-lint rules=obsidian:ui.settings-tab
// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingTab(tab);

// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this["addSettingTab"](secondTab);
this.addSettingTab = replacement;
// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingTab(thirdTab);

import { PluginSettingTab as pluginSettingTab } from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
class Settings extends pluginSettingTab {
  getSettingDefinitions() { return []; }
}

// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
new settingsNamespace.PluginSettingTab();
// @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
new CommonJsPluginSettingTab();

import * as settingsNamespace from "obsidian";
const { PluginSettingTab: CommonJsPluginSettingTab } = require("obsidian");
