// @case description exact Obsidian link helpers through module provenance
// @tool glass-lint rules=obsidian:markdown.link
import { parseLinktext } from "obsidian";
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseLinktext(text);

import { normalizePath as normalize } from 'obsidian';
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
normalize(path);

import * as obsidian from 'obsidian';
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
obsidian.getLinkpath(link);

// Destructured CommonJS exports retain the same module provenance.
const { parseLinktext: parseCommonJs } = require('obsidian');
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseCommonJs(otherText);
