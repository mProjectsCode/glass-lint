// @case description negative fixture for obsidian:markdown.link
// @tool glass-lint rules=obsidian:markdown.link
// @expect-no-error glass-lint rule=obsidian:markdown.link message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:markdown.link message_id=detected
import { parseLinktext as localParse } from "markdown-utils";
localParse(text);
// Migrated: interface/local-classes-ignored.js and unused-imports-ignored.js
import { MarkdownView as unusedMarkdownView } from "obsidian";
class localMarkdownView {}
new localMarkdownView();
unusedMarkdownView;
