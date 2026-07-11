// @case description A plugin reads metadata for the active file
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=recommended
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:lifecycle.events count=1 line=any
// @expect-error glass-lint rule=obsidian:metadata.cache-read count=2 line=any
// @expect-error glass-lint rule=obsidian:metadata.events count=1 line=any
// @expect-error glass-lint rule=obsidian:workspace.active-file count=1 line=any

import { Plugin } from "obsidian";

export default class MetadataPlugin extends Plugin {
  onload() {
    this.tagCounts = new Map();
    this.addCommand({
      id: "inspect-active-tags",
      name: "Inspect active note tags",
      callback: () => this.inspectActiveFile(),
    });
    this.registerEvent(this.app.metadataCache.on("changed", (file) => this.inspect(file)));
  }

  inspectActiveFile() {
    const file = this.app.workspace.getActiveFile();
    if (file) this.inspect(file);
  }

  inspect(file) {
    const cache = this.app.metadataCache.getFileCache(file);
    const tags = this.collectTags(cache);
    this.tagCounts.set(file.path, tags.length);
    console.log(`${file.path}: ${tags.join(", ")}`);
  }

  collectTags(cache) {
    const result = new Set();
    for (const entry of cache?.tags ?? []) {
      result.add(entry.tag);
    }
    const frontmatterTags = cache?.frontmatter?.tags;
    if (Array.isArray(frontmatterTags)) {
      for (const tag of frontmatterTags) result.add(`#${tag}`);
    } else if (typeof frontmatterTags === "string") {
      result.add(`#${frontmatterTags}`);
    }
    return Array.from(result).sort();
  }

  onunload() {
    this.tagCounts.clear();
  }
}
