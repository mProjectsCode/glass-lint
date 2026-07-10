// @case description Obsidian UI class references report their corresponding rules
// @tool glass-lint rules=obsidian:ui.modal,obsidian:ui.notice,obsidian:ui.settings-tab
// @tool eslint-obsidianmd config=recommended

import { Modal, Notice, PluginSettingTab } from "obsidian";

class ExampleModal extends Modal {} // @expect-error glass-lint rule=obsidian:ui.modal message_id=detected line=any column=any
const show = () => new Notice("done"); // @expect-error glass-lint rule=obsidian:ui.notice message_id=detected

class ExamplePlugin {
  async onload() {}
  onunload() {}
}

class ExampleSettings extends PluginSettingTab { // @expect-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
  getSettingDefinitions() {
    return [];
  }
}
