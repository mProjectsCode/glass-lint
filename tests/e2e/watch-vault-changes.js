// @case description A plugin subscribes to vault changes through lifecycle cleanup
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=recommended
// @expect-error glass-lint rule=obsidian:lifecycle.events count=3 line=any
// @expect-error glass-lint rule=obsidian:vault.access count=3 line=any
// @expect-error glass-lint rule=obsidian:vault.events count=3 line=any

import { Plugin } from "obsidian";

export default class VaultWatcherPlugin extends Plugin {
  onload() {
    this.changedPaths = new Set();
    this.flushTimer = null;
    this.registerEvent(
      this.app.vault.on("modify", (file) => this.record(file)),
    );
    this.registerEvent(this.app.vault.on("delete", (file) => this.forget(file)));
    this.registerEvent(this.app.vault.on("rename", (file, oldPath) => this.rename(file, oldPath)));
  }

  record(file) {
    if (file.extension !== "md") return;
    this.changedPaths.add(file.path);
    this.scheduleFlush();
  }

  forget(file) {
    this.changedPaths.delete(file.path);
  }

  rename(file, oldPath) {
    this.changedPaths.delete(oldPath);
    this.record(file);
  }

  scheduleFlush() {
    if (this.flushTimer !== null) window.clearTimeout(this.flushTimer);
    this.flushTimer = window.setTimeout(() => this.flush(), 250);
  }

  flush() {
    const paths = Array.from(this.changedPaths).sort();
    if (paths.length > 0) console.log("Changed notes:", paths.join(", "));
    this.changedPaths.clear();
    this.flushTimer = null;
  }

  onunload() {
    if (this.flushTimer !== null) window.clearTimeout(this.flushTimer);
    this.changedPaths.clear();
  }
}
