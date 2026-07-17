// @case description A plugin can make a browser network request
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=recommended
// @expect-error glass-lint rule=browser:network.request count=1 line=any
// @expect-error glass-lint rule=js:network.url-construction count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.status-bar count=1 line=any

import { Plugin } from "obsidian";

export default class NetworkPlugin extends Plugin {
  async onload() {
    this.abortController = new AbortController();
    this.addCommand({
      id: "refresh-catalog",
      name: "Refresh remote catalog",
      callback: () => this.refresh(),
    });
    await this.refresh();
  }

  async refresh() {
    const endpoint = this.buildEndpoint("catalog");
    const response = await fetch(endpoint, {
      headers: { Accept: "application/json" },
      signal: this.abortController.signal,
    });
    if (!response.ok) {
      throw new Error(`Catalog request failed: ${response.status}`);
    }
    const records = await response.json();
    this.renderSummary(records);
  }

  buildEndpoint(resource) {
    const base = "https://example.com/api";
    const url = new URL(`${base}/${resource}`);
    url.searchParams.set("client", "glass-lint-e2e");
    return url.toString();
  }

  renderSummary(records) {
    const status = this.addStatusBarItem();
    const count = Array.isArray(records) ? records.length : 0;
    status.setText(`${count} catalog entries`);
  }

  onunload() {
    this.abortController.abort();
  }
}
