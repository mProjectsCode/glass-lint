// @case description direct reads and rooted aliases
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
app.metadataCache.getFileCache.frontmatter;
const cacheResult = app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
cacheResult.frontmatter;
const metadataCache = this.app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
metadataCache.frontmatter;

// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
const frontmatter = app.metadataCache.getFileCache.frontmatter;
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read
frontmatter;

// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
app.metadataCache.getFileCache.frontmatter;

// Static computed properties retain the rooted chain.
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
app['metadataCache']['getFileCache']['frontmatter'];
import { parseFrontMatterAliases, parseFrontMatterTags } from "obsidian";
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
parseFrontMatterAliases(frontmatter);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read
parseFrontMatterTags(frontmatter);
