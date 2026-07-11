// @case description A plugin creates a note in the vault
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:vault.access count=4 line=any
// @expect-error glass-lint rule=obsidian:vault.write count=2 line=any
// @expect-error glass-lint rule=obsidian:workspace.open count=2 line=any

import { Plugin } from "obsidian";

export default class NoteCreatorPlugin extends Plugin {
  async onload() {
    this.addCommand({
      id: "create-meeting-note",
      name: "Create meeting note",
      callback: () => this.createMeetingNote(new Date()),
    });
  }

  async createMeetingNote(now) {
    const folder = "Meetings";
    await this.ensureFolder(folder);
    const path = `${folder}/${this.dateStamp(now)}.md`;
    const existing = this.app.vault.getAbstractFileByPath(path);
    if (existing) {
      await this.app.workspace.getLeaf(false).openFile(existing);
      return;
    }

    const body = this.template(now);
    const file = await this.app.vault.create(path, body);
    await this.app.workspace.getLeaf(false).openFile(file);
  }

  async ensureFolder(path) {
    if (!this.app.vault.getAbstractFileByPath(path)) {
      await this.app.vault.createFolder(path);
    }
  }

  dateStamp(date) {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${year}-${month}-${day}`;
  }

  template(date) {
    return [
      "---",
      `created: ${date.toISOString()}`,
      "tags: [meeting]",
      "---",
      "",
      "# Meeting",
      "",
      "## Attendees",
      "",
      "## Notes",
      "",
      "## Actions",
      "",
    ].join("\n");
  }
}
