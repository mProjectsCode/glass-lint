// @case description Static Node, Electron, and browser risk APIs are detected
// @tool glass-lint rules=obsidian:ui.file_dialog,obsidian:workspace.layout_persistence,obsidian:plugins.internal_access,obsidian:metadata.events,obsidian:metadata.extraction,obsidian:browser.broad_input_hooks,obsidian:network.remote_dom_loading,obsidian:vault.enumerate

import { dialog } from "electron";
import { requestUrl } from "obsidian";
dialog.showOpenDialog({ properties: ["openFile"] }); // @expect-error glass-lint rule=obsidian:ui.file_dialog message_id=detected
this.app.workspace.requestSaveLayout(); // @expect-error glass-lint rule=obsidian:workspace.layout_persistence message_id=detected
this.app.plugins.getPlugin("dataview"); // @expect-error glass-lint rule=obsidian:plugins.internal_access message_id=detected
this.app.metadataCache.on("changed", () => {}); // @expect-error glass-lint rule=obsidian:metadata.events message_id=detected
const file = this.app.workspace.getActiveFile();
const cache = this.app.metadataCache.getFileCache(file);
const tags = cache.tags; // @expect-error glass-lint rule=obsidian:metadata.extraction message_id=detected
const links = cache.links; // @expect-error glass-lint rule=obsidian:metadata.extraction message_id=detected
const embeds = cache.embeds; // @expect-error glass-lint rule=obsidian:metadata.extraction message_id=detected
document.addEventListener("keydown", () => {}); // @expect-error glass-lint rule=obsidian:browser.broad_input_hooks message_id=detected
const script = document.createElement("script"); // @expect-error glass-lint rule=obsidian:network.remote_dom_loading message_id=detected
script.src = "https://cdn.example.com/plugin.js";
document.head.appendChild(script);
const img = document.createElement("img"); // @expect-error glass-lint rule=obsidian:network.remote_dom_loading message_id=detected
img.src = "https://cdn.example.com/logo.png";
document.body.appendChild(img);
const link = document.createElement("link"); // @expect-error glass-lint rule=obsidian:network.remote_dom_loading message_id=detected
link.rel = "stylesheet";
link.href = "https://cdn.example.com/theme.css";
document.head.appendChild(link);
const style = document.createElement("style"); // @expect-error glass-lint rule=obsidian:network.remote_dom_loading message_id=detected
style.textContent = "@import url('https://cdn.example.com/theme.css')";
document.head.appendChild(style);
await requestUrl("https://example.com");
this.app.vault.getFiles(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
