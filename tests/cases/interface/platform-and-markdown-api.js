// @case description Ported old classifier cases: platform branching and markdown class references
// @tool glass-lint rules=obsidian:platform.branching,obsidian:editor.markdown_api

import * as obsidian from "obsidian";
import { MarkdownView } from "obsidian"; // @expect-error glass-lint rule=obsidian:editor.markdown_api message_id=detected line=14

this?.app?.vault?.["re" + "ad"]?.(file);
obsidian?.Platform?.[`isMobile`]; // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected

if (obsidian . Platform ["isMobile"]) { // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
  console.log("mobile");
}

if (leaf.view instanceof MarkdownView) {
  leaf.view.editor.getValue();
}
