// @case description Ported old classifier case: representative Obsidian API groups
// @tool glass-lint rules=obsidian:network.obsidian,obsidian:vault.read,obsidian:vault.enumerate,obsidian:metadata.read,obsidian:metadata.frontmatter,obsidian:workspace.active_file,obsidian:ui.commands,obsidian:editor.extension,obsidian:editor.markdown_processing,obsidian:lifecycle.events

import { requestUrl, MarkdownRenderer } from "obsidian"; // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected line=12

class Plugin {
  async onload() {
    this.addCommand({ id: "x", callback: () => {} }); // @expect-error glass-lint rule=obsidian:ui.commands message_id=detected
    this.registerEditorExtension([]); // @expect-error glass-lint rule=obsidian:editor.extension message_id=detected
    this.registerMarkdownPostProcessor(() => {}); // @expect-error glass-lint rule=obsidian:editor.markdown_processing message_id=detected
    this.registerInterval(setInterval(() => {}, 1000)); // @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected count=2
    await requestUrl("https://example.com");
    const file = this.app.workspace.getActiveFile(); // @expect-error glass-lint rule=obsidian:workspace.active_file message_id=detected
    const text = await this.app.vault.read(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
    this.app.vault.getMarkdownFiles(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
    const cache = this.app.metadataCache.getFileCache(file); // @expect-error glass-lint rule=obsidian:metadata.read message_id=detected
    cache.frontmatter; // @expect-error glass-lint rule=obsidian:metadata.frontmatter message_id=detected
    MarkdownRenderer.render(this.app, text, this.containerEl, "", this); // @expect-error glass-lint rule=obsidian:editor.markdown_processing message_id=detected
  }
}
