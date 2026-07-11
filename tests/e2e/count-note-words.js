// @case description A plugin reads the active file from its vault
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:lifecycle.events count=1 line=any
// @expect-error glass-lint rule=obsidian:vault.access count=2 line=any
// @expect-error glass-lint rule=obsidian:vault.events count=1 line=any
// @expect-error glass-lint rule=obsidian:vault.read count=1 line=any
// @expect-error glass-lint rule=obsidian:workspace.active-file count=1 line=any

import { Plugin } from "obsidian";

export default class VaultReaderPlugin extends Plugin {
  async onload() {
    this.wordsByPath = new Map();
    this.addCommand({
      id: "count-active-note",
      name: "Count words in active note",
      checkCallback: (checking) => this.countActiveNote(checking),
    });
    this.registerEvent(this.app.vault.on("modify", (file) => this.forget(file)));
  }

  async countActiveNote(checking) {
    const file = this.app.workspace.getActiveFile();
    if (!file || file.extension !== "md") return false;
    if (checking) return true;

    const count = await this.countWords(file);
    console.log(`${file.path}: ${count} words`);
    return true;
  }

  async countWords(file) {
    if (this.wordsByPath.has(file.path)) {
      return this.wordsByPath.get(file.path);
    }
    const contents = await this.app.vault.cachedRead(file);
    const count = contents
      .trim()
      .split(/\s+/u)
      .filter(Boolean).length;
    this.wordsByPath.set(file.path, count);
    return count;
  }

  forget(file) {
    this.wordsByPath.delete(file.path);
  }

  onunload() {
    this.wordsByPath.clear();
  }
}
