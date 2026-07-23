// @case description exact Obsidian link helpers through module provenance
// @tool glass-lint rules=obsidian:markdown.link
import { parseLinktext } from "obsidian";
// @expect-error glass-lint rule=obsidian:markdown.link
parseLinktext(text);

import { normalizePath as normalize } from 'obsidian';
// @expect-error glass-lint rule=obsidian:markdown.link
normalize(path);

import * as obsidian from 'obsidian';
// @expect-error glass-lint rule=obsidian:markdown.link
obsidian.getLinkpath(link);

// Destructured CommonJS exports retain the same module provenance.
const { parseLinktext: parseCommonJs } = require('obsidian');
// @expect-error glass-lint rule=obsidian:markdown.link
parseCommonJs(otherText);
import {
  fileToLinktext,
  generateMarkdownLink,
  resolveSubpath,
  parseSubpath,
  parseFrontMatterAliases,
  parseFrontMatterTags,
} from "obsidian";
// @expect-error glass-lint rule=obsidian:markdown.link
fileToLinktext(file, sourcePath);
// @expect-error glass-lint rule=obsidian:markdown.link
generateMarkdownLink(file, sourcePath);
// @expect-error glass-lint rule=obsidian:markdown.link
resolveSubpath(path);
// @expect-error glass-lint rule=obsidian:markdown.link
parseSubpath(path);
// Frontmatter helpers are part of the same exact module export set.
// @expect-error glass-lint rule=obsidian:markdown.link
parseFrontMatterAliases(frontmatter);
// @expect-error glass-lint rule=obsidian:markdown.link
parseFrontMatterTags(frontmatter);
