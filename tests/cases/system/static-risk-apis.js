// @case description Static Node, Electron, and browser risk APIs are detected
// @tool glass-lint rules=obsidian:workspace.layout,obsidian:plugins.other-access,obsidian:metadata.events,obsidian:metadata.extract,js:browser.global-input-hook,js:dom.remote-resource,obsidian:vault.enumerate
// @tool eslint-obsidianmd config=recommended

import { dialog } from "electron";
import { requestUrl } from "obsidian";
dialog.showOpenDialog({ properties: ["openFile"] });
this.app.workspace.requestSaveLayout(); // @expect-error glass-lint rule=obsidian:workspace.layout message_id=detected
this.app.plugins.getPlugin("dataview"); // @expect-error glass-lint rule=obsidian:plugins.other-access message_id=detected
this.app.metadataCache.on("changed", () => {}); // @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
const file = this.app.workspace.getActiveFile();
const cache = this.app.metadataCache.getFileCache(file);
const tags = cache.tags; // @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
const links = cache.links; // @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
const embeds = cache.embeds; // @expect-error glass-lint rule=obsidian:metadata.extract message_id=detected
document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=js:browser.global-input-hook message_id=detected
const script = document.createElement("script"); // @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
script.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(script);
const img = document.createElement("img"); // @expect-error glass-lint rule=js:dom.remote-resource message_id=detected
img.src = "https://cdn.example.com/logo.png";
document.body.appendChild(img);
const link = document.createElement("link");
link.rel = "stylesheet";
link.href = "https://cdn.example.com/theme.css";
document.head.appendChild(link);
const style = document.createElement("style");
style.textContent = "@import url('https://cdn.example.com/theme.css')";
document.head.appendChild(style);
await requestUrl("https://example.com");
this.app.vault.getFiles(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
