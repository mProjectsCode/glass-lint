# Glass Lint vs ESLint comparison

| Case | eslint-obsidianmd | glass-lint |
|---|---:|---:|
| count-note-words | 0 | 7 |
| create-meeting-note | 0 | 9 |
| download-daily-quote | 0 | 2 |
| fetch-remote-catalog | 0 | 4 |
| inspect-note-tags | 0 | 6 |
| open-workspace-links | 0 | 4 |
| persist-refresh-settings | 0 | 4 |
| roll-ribbon-dice | 0 | 3 |
| transform-text-case | 0 | 1 |
| watch-vault-changes | 0 | 9 |

## count-note-words

```js
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (7 finding(s))
- obsidian:ui.command:detected at 16:5 - Registers commands
- obsidian:lifecycle.events:detected at 21:5 - Registers Obsidian lifecycle events
- obsidian:vault.access:detected at 21:24 - Accesses Obsidian vault APIs
- obsidian:vault.events:detected at 21:24 - Registers vault events
- obsidian:workspace.active-file:detected at 25:18 - Accesses the active file
- obsidian:vault.access:detected at 38:28 - Accesses Obsidian vault APIs
- obsidian:vault.read:detected at 38:28 - Reads vault files

## create-meeting-note

```js
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (9 finding(s))
- obsidian:ui.command:detected at 13:5 - Registers commands
- obsidian:vault.access:detected at 24:22 - Accesses Obsidian vault APIs
- obsidian:workspace.open:detected at 26:13 - Opens files through the workspace
- obsidian:vault.access:detected at 31:24 - Accesses Obsidian vault APIs
- obsidian:vault.write:detected at 31:24 - Writes vault files
- obsidian:workspace.open:detected at 32:11 - Opens files through the workspace
- obsidian:vault.access:detected at 36:10 - Accesses Obsidian vault APIs
- obsidian:vault.access:detected at 37:13 - Accesses Obsidian vault APIs
- obsidian:vault.write:detected at 37:13 - Writes vault files

## download-daily-quote

```js
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (2 finding(s))
- obsidian:ui.command:detected at 12:5 - Registers commands
- obsidian:network.request:detected at 22:28 - Uses Obsidian request APIs

## fetch-remote-catalog

```js
// @case description A plugin can make a browser network request
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=js:network.request count=1 line=any
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (4 finding(s))
- js:network.request:detected at 24:28 - Uses browser network request APIs
- js:network.url-construction:detected at 37:21 - Constructs or references URLs
- obsidian:ui.command:detected at 14:5 - Registers commands
- obsidian:ui.status-bar:detected at 43:20 - Registers status bar items

## inspect-note-tags

```js
// @case description A plugin reads metadata for the active file
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (6 finding(s))
- obsidian:ui.command:detected at 15:5 - Registers commands
- obsidian:lifecycle.events:detected at 20:5 - Registers Obsidian lifecycle events
- obsidian:metadata.cache-read:detected at 20:24 - Reads Obsidian metadata cache
- obsidian:metadata.events:detected at 20:24 - Registers metadata cache events
- obsidian:workspace.active-file:detected at 24:18 - Accesses the active file
- obsidian:metadata.cache-read:detected at 29:19 - Reads Obsidian metadata cache

## open-workspace-links

```js
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (4 finding(s))
- obsidian:ui.command:detected at 13:5 - Registers commands
- obsidian:ui.command:detected at 18:5 - Registers commands
- obsidian:workspace.open:detected at 28:11 - Opens files through the workspace
- obsidian:workspace.active-file:detected at 39:18 - Accesses the active file

## persist-refresh-settings

```js
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (4 finding(s))
- obsidian:storage.plugin-data-read:detected at 13:62 - Reads plugin data
- obsidian:ui.command:detected at 14:5 - Registers commands
- obsidian:storage.plugin-data-write:detected at 37:11 - Writes plugin data
- obsidian:lifecycle.events:detected at 45:5 - Registers Obsidian lifecycle events

## roll-ribbon-dice

```js
// @case description A plugin adds a ribbon action
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.status-bar count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.ribbon count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any

import { Plugin } from "obsidian";

export default class RibbonPlugin extends Plugin {
  onload() {
    this.rolls = [];
    this.status = this.addStatusBarItem();
    this.ribbon = this.addRibbonIcon("dice", "Roll dice", () => this.roll());
    this.ribbon.addClass("dice-roller-ribbon");
    this.addCommand({
      id: "roll-dice",
      name: "Roll dice",
      callback: () => this.roll(),
    });
    this.updateStatus();
  }

  roll() {
    const value = this.randomInt(1, 6);
    this.rolls.push({ value, time: Date.now() });
    if (this.rolls.length > 20) this.rolls.shift();
    this.updateStatus();
    return value;
  }

  randomInt(minimum, maximum) {
    const span = maximum - minimum + 1;
    return minimum + Math.floor(Math.random() * span);
  }

  updateStatus() {
    const latest = this.rolls.at(-1);
    if (!latest) {
      this.status.setText("Dice: not rolled");
      return;
    }
    const average = this.rolls.reduce((sum, roll) => sum + roll.value, 0) / this.rolls.length;
    this.status.setText(`Dice: ${latest.value} (avg ${average.toFixed(1)})`);
  }

  onunload() {
    this.rolls.length = 0;
    this.status = null;
  }
}

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (3 finding(s))
- obsidian:ui.status-bar:detected at 13:19 - Registers status bar items
- obsidian:ui.ribbon:detected at 14:19 - Registers ribbon icons
- obsidian:ui.command:detected at 16:5 - Registers commands

## transform-text-case

```js
// @case description A plugin registers a command
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any

import { Plugin } from "obsidian";

export default class CommandPlugin extends Plugin {
  onload() {
    this.history = [];
    this.registerCommands();
  }

  registerCommands() {
    const commands = [
      ["uppercase-selection", "Uppercase selection", (editor) => this.uppercase(editor)],
      ["lowercase-selection", "Lowercase selection", (editor) => this.lowercase(editor)],
      ["repeat-transform", "Repeat last transform", (editor) => this.repeat(editor)],
    ];
    for (const [id, name, editorCallback] of commands) {
      this.addCommand({ id, name, editorCallback });
    }
  }

  uppercase(editor) {
    this.transform(editor, "uppercase", (text) => text.toUpperCase());
  }

  lowercase(editor) {
    this.transform(editor, "lowercase", (text) => text.toLowerCase());
  }

  transform(editor, name, operation) {
    const source = editor.getSelection();
    if (!source) return;
    const result = operation(source);
    editor.replaceSelection(result);
    this.history.push({ name, source, result });
    if (this.history.length > 10) this.history.shift();
  }

  repeat(editor) {
    const previous = this.history.at(-1);
    if (!previous) return;
    if (previous.name === "uppercase") this.uppercase(editor);
    if (previous.name === "lowercase") this.lowercase(editor);
  }

  onunload() {
    this.history.length = 0;
  }
}

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (1 finding(s))
- obsidian:ui.command:detected at 21:7 - Registers commands

## watch-vault-changes

```js
// @case description A plugin subscribes to vault changes through lifecycle cleanup
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
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

```

### eslint-obsidianmd (0 finding(s))

### glass-lint (9 finding(s))
- obsidian:lifecycle.events:detected at 14:5 - Registers Obsidian lifecycle events
- obsidian:vault.access:detected at 15:7 - Accesses Obsidian vault APIs
- obsidian:vault.events:detected at 15:7 - Registers vault events
- obsidian:lifecycle.events:detected at 17:5 - Registers Obsidian lifecycle events
- obsidian:vault.access:detected at 17:24 - Accesses Obsidian vault APIs
- obsidian:vault.events:detected at 17:24 - Registers vault events
- obsidian:lifecycle.events:detected at 18:5 - Registers Obsidian lifecycle events
- obsidian:vault.access:detected at 18:24 - Accesses Obsidian vault APIs
- obsidian:vault.events:detected at 18:24 - Registers vault events
