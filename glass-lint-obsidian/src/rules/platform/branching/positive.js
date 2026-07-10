// @case description positive fixture for obsidian:platform.branching
// @tool glass-lint rules=obsidian:platform.branching
import * as obsidian from "obsidian";
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isMobile;
// second independent example
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isDesktop;

// Migrated: interface/platform-and-markdown-api.js
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian?.Platform?.[`isMobile`];
if (obsidian.Platform["isMobile"]) { // @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
  console.log("mobile");
}
