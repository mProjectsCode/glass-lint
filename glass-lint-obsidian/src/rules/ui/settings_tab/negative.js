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

import { Plugin } from "obsidian";
class ReassignedPlugin extends Plugin {
  run() {
    this.addSettingTab = replacement;
    // @expect-no-error glass-lint rule=obsidian:ui.settings-tab
    this.addSettingTab(thirdTab);
  }
}

function shadowed(LocalSettingTab) {
  // @expect-no-error glass-lint rule=obsidian:ui.settings-tab
  new LocalSettingTab();
}

class LocalSettingTab {}
import { PluginSettingTab as ImportedSettingTab } from "obsidian";
let reassigned = ImportedSettingTab;
reassigned = LocalSettingTab;
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab
new reassigned();
