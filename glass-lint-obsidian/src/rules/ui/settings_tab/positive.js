// @case description registration and module-provenance settings tabs
// @tool glass-lint rules=obsidian:ui.settings-tab
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:ui.settings-tab
this.addSettingTab(tab);

// @expect-error glass-lint rule=obsidian:ui.settings-tab
this["addSettingTab"](secondTab);
this.addSettingTab = replacement;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
this.addSettingTab(thirdTab);
  }
}

import { PluginSettingTab as pluginSettingTab } from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.settings-tab
class Settings extends pluginSettingTab {
  getSettingDefinitions() { return []; }
}

// @expect-error glass-lint rule=obsidian:ui.settings-tab
new settingsNamespace.PluginSettingTab();
// @expect-error glass-lint rule=obsidian:ui.settings-tab
new CommonJsPluginSettingTab();

import * as settingsNamespace from "obsidian";
const { PluginSettingTab: CommonJsPluginSettingTab } = require("obsidian");
