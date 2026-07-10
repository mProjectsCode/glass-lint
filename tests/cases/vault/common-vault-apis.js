// @case description Common Obsidian vault API groups are detected
// @tool glass-lint rules=obsidian:network.request,obsidian:vault.read,obsidian:vault.enumerate,obsidian:metadata.cache-read,obsidian:metadata.frontmatter-read,obsidian:workspace.active-file,obsidian:ui.command,obsidian:editor.extension,obsidian:markdown.render,obsidian:lifecycle.events
// @tool eslint-obsidianmd config=recommended

import { requestUrl, MarkdownRenderer } from "obsidian";

class Plugin {
  async onload() {
    this.addCommand({ id: "x", callback: () => {} }); // @expect-error glass-lint rule=obsidian:ui.command message_id=detected
    this.registerEditorExtension([]); // @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
    this.registerMarkdownPostProcessor(() => {}); // @expect-error glass-lint rule=obsidian:markdown.render message_id=detected line=any column=any
    this.registerInterval(setInterval(() => {}, 1000)); // @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected count=1
    await requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.request message_id=detected
    const file = this.app.workspace.getActiveFile(); // @expect-error glass-lint rule=obsidian:workspace.active-file message_id=detected
    const text = await this.app.vault.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
    this.app.vault.getMarkdownFiles(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
    const cache = this.app.metadataCache.getFileCache(file); // @expect-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
    cache.frontmatter; // @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
    MarkdownRenderer.render(this.app, text, this.containerEl, "", this); // @expect-error glass-lint rule=obsidian:markdown.render message_id=detected line=any column=any
  }
}
