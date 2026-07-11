// @case description A plugin uses Obsidian's network request API
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:network.request count=1 line=any

import { Plugin, requestUrl } from "obsidian";

export default class RequestPlugin extends Plugin {
  async onload() {
    this.cache = new Map();
    this.addCommand({
      id: "download-daily-quote",
      name: "Download daily quote",
      callback: () => this.downloadQuote(),
    });
  }

  async downloadQuote() {
    const day = new Date().toISOString().slice(0, 10);
    if (this.cache.has(day)) return this.cache.get(day);
    const response = await requestUrl({
      url: `https://example.com/quotes/${day}.json`,
      method: "GET",
      headers: { Accept: "application/json" },
      throw: false,
    });
    if (response.status !== 200) {
      console.warn(`Quote service returned ${response.status}`);
      return null;
    }
    const quote = this.normalize(response.json);
    this.cache.set(day, quote);
    return quote;
  }

  normalize(payload) {
    if (!payload || typeof payload.text !== "string") return null;
    return {
      text: payload.text.trim(),
      author: String(payload.author ?? "Unknown").trim(),
    };
  }

  hasCachedQuote(day) {
    return this.cache.has(day);
  }

  onunload() {
    this.cache.clear();
  }
}
