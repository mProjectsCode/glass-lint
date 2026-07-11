// @case description A plugin opens a link in the workspace
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.command count=2 line=any
// @expect-error glass-lint rule=obsidian:workspace.open count=1 line=any
// @expect-error glass-lint rule=obsidian:workspace.active-file count=1 line=any

import { Plugin } from "obsidian";

export default class LinkOpenerPlugin extends Plugin {
  async onload() {
    this.destinations = ["Home", "Daily note", "Projects"];
    this.addCommand({
      id: "open-home",
      name: "Open home note",
      callback: () => this.open("Home"),
    });
    this.addCommand({
      id: "open-next-destination",
      name: "Open next destination",
      callback: () => this.openNext(),
    });
    this.index = 0;
  }

  async open(name) {
    const source = this.activePath();
    await this.app.workspace.openLinkText(name, source, false);
    this.remember(name);
  }

  async openNext() {
    const name = this.destinations[this.index];
    this.index = (this.index + 1) % this.destinations.length;
    await this.open(name);
  }

  activePath() {
    const file = this.app.workspace.getActiveFile();
    return file ? file.path : "";
  }

  remember(name) {
    const previous = this.destinations.indexOf(name);
    if (previous >= 0) this.destinations.splice(previous, 1);
    this.destinations.unshift(name);
  }

  onunload() {
    this.destinations.length = 0;
  }
}
