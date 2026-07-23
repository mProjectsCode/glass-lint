// @case description similar modules, shadowed loads, and reassigned aliases
// @tool glass-lint rules=obsidian:markdown.link
// @expect-no-error glass-lint rule=obsidian:markdown.link
import { parseLinktext as localParse } from 'markdown-utils';
localParse(text);

// @expect-no-error glass-lint rule=obsidian:markdown.link
import { parseLinktext as unrelatedParse } from 'obsidian-utils';
unrelatedParse(text);

// A shadowed CommonJS loader is not the Obsidian module.
function require(name) { return { parseLinktext() {} }; }
// @expect-no-error glass-lint rule=obsidian:markdown.link
require('obsidian').parseLinktext(text);

// Dynamic module names cannot be assigned Obsidian provenance.
const moduleName = 'obsidian';
// @expect-no-error glass-lint rule=obsidian:markdown.link
require(moduleName).normalizePath(path);

// A proven export stops matching after its alias is reassigned.
import { getLinkpath } from 'obsidian';
let getPath = getLinkpath;
getPath = localGetPath;
// @expect-no-error glass-lint rule=obsidian:markdown.link
getPath(link);

// The configured exports are not inferred from unrelated local objects.
const localObsidian = { parseLinktext() {} };
// @expect-no-error glass-lint rule=obsidian:markdown.link
localObsidian.parseLinktext(text);

// Similar frontmatter helpers from another package are not Obsidian exports.
import { parseFrontMatterTags as localTags } from 'markdown-utils';
// @expect-no-error glass-lint rule=obsidian:markdown.link
localTags(frontmatter);
unusedMarkdownView;
