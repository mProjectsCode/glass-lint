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
import {
  fileToLinktext,
  generateMarkdownLink,
  resolveSubpath,
  parseSubpath,
  parseFrontMatterAliases,
  parseFrontMatterTags,
} from "obsidian";
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
fileToLinktext(file, sourcePath);
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
generateMarkdownLink(file, sourcePath);
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
resolveSubpath(path);
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseSubpath(path);
// Frontmatter helpers are part of the same exact module export set.
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseFrontMatterAliases(frontmatter);
// @expect-error glass-lint rule=obsidian:markdown.link message_id=detected
parseFrontMatterTags(frontmatter);
