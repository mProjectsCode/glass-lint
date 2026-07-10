// @case description Platform branching and Markdown API class references are detected
// @tool glass-lint rules=obsidian:platform.branching,obsidian:markdown.link,obsidian:vault.read
// @tool eslint-obsidianmd config=recommended

import * as obsidian from "obsidian";
import { MarkdownView } from "obsidian";

this?.app?.vault?.["re" + "ad"]?.(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
obsidian?.Platform?.[`isMobile`]; // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected

if (obsidian . Platform ["isMobile"]) { // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
  console.log("mobile");
}

if (leaf.view instanceof MarkdownView) {
  leaf.view.editor.getValue();
}
