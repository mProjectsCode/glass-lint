// @case description all configured metadata collections and rooted aliases
// @tool glass-lint rules=obsidian:metadata.extract
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.tags;
const cacheResult = app.metadataCache.getFileCache(file);
const cacheResultAlias = cacheResult;
// @expect-error glass-lint rule=obsidian:metadata.extract
cacheResultAlias.links;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.links;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.embeds;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.blocks;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.headings;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.sections;

// @expect-error glass-lint rule=obsidian:metadata.extract
const metadataLinks = app.metadataCache.getFileCache.links;
// @expect-no-error glass-lint rule=obsidian:metadata.extract
metadataLinks;

const extractCache = this.app.metadataCache.getFileCache;
// @expect-error glass-lint rule=obsidian:metadata.extract
extractCache.tags;
// @expect-error glass-lint rule=obsidian:metadata.extract
extractCache.links;
// @expect-error glass-lint rule=obsidian:metadata.extract
extractCache.embeds;

// Static computed properties remain rooted.
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache['sections'];
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.listItems;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.footnotes;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.footnoteRefs;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.referenceLinks;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatterLinks;
// Frontmatter itself and its derived aliases/tags are cached metadata too.
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatter;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatterAliases;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatterTags;
// @expect-error glass-lint rule=obsidian:metadata.extract
app.metadataCache.getFileCache.frontmatterPosition;
