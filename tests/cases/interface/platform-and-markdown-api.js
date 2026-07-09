// @case description Platform branching and Markdown API class references are detected
// @tool glass-lint rules=obsidian:platform.branching,obsidian:editor.markdown_api,obsidian:vault.read

import * as obsidian from "obsidian";
import { MarkdownView } from "obsidian"; // @expect-error glass-lint rule=obsidian:editor.markdown_api message_id=detected line=14

this?.app?.vault?.["re" + "ad"]?.(file); // @expect-error glass-lint rule=obsidian:vault.read message_id=detected
obsidian?.Platform?.[`isMobile`]; // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected

if (obsidian . Platform ["isMobile"]) { // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
  console.log("mobile");
}

if (leaf.view instanceof MarkdownView) {
  leaf.view.editor.getValue();
}
