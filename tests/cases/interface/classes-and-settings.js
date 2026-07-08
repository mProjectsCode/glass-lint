// @case description Ported old classifier cases: Obsidian UI class references
// @tool glass-lint rules=obsidian:ui.modals_notices,obsidian:lifecycle.methods,obsidian:settings.ui

import { Modal, Notice, PluginSettingTab } from "obsidian"; // @expect-error glass-lint rule=obsidian:ui.modals_notices message_id=detected count=2

class ExampleModal extends Modal {}
const show = () => new Notice("done");

class ExamplePlugin {
  async onload() {} // @expect-error glass-lint rule=obsidian:lifecycle.methods message_id=detected
  onunload() {} // @expect-error glass-lint rule=obsidian:lifecycle.methods message_id=detected
}

class ExampleSettings extends PluginSettingTab { // @expect-error glass-lint rule=obsidian:settings.ui message_id=detected line=4
  getSettingDefinitions() {
    return [];
  }
}
