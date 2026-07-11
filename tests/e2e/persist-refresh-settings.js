// @case description A plugin persists its own settings
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read count=1 line=any
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:lifecycle.events count=1 line=any

import { Plugin } from "obsidian";

export default class SettingsPlugin extends Plugin {
  async onload() {
    this.settings = Object.assign({}, this.defaults(), await this.loadData());
    this.addCommand({
      id: "toggle-auto-refresh",
      name: "Toggle automatic refresh",
      callback: () => this.toggleRefresh(),
    });
    this.scheduleRefresh();
  }

  defaults() {
    return {
      enabled: true,
      refreshMinutes: 15,
      lastRefresh: null,
    };
  }

  async toggleRefresh() {
    this.settings.enabled = !this.settings.enabled;
    await this.persist();
    this.scheduleRefresh();
  }

  async persist() {
    await this.saveData(this.settings);
  }

  scheduleRefresh() {
    if (this.timer !== undefined) window.clearInterval(this.timer);
    if (!this.settings.enabled) return;
    const milliseconds = this.settings.refreshMinutes * 60 * 1000;
    this.timer = window.setInterval(() => this.markRefreshed(), milliseconds);
    this.registerInterval(this.timer);
  }

  async markRefreshed() {
    this.settings.lastRefresh = new Date().toISOString();
    await this.persist();
  }

  onunload() {
    if (this.timer !== undefined) window.clearInterval(this.timer);
  }
}
