// @case description positive fixture for obsidian:markdown.link
// @tool glass-lint rules=obsidian:markdown.link
import { parseLinktext } from "obsidian";

// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseLinktext(text);
// second independent example

// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseLinktext(secondText);
