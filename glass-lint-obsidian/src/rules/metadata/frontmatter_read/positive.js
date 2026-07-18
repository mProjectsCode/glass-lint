// @case description direct reads and rooted aliases
// @tool glass-lint rules=obsidian:metadata.frontmatter-read
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;
const cacheResult = app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
cacheResult.frontmatter;
const metadataCache = this.app.metadataCache.getFileCache(file);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
metadataCache.frontmatter;

// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
const frontmatter = app.metadataCache.getFileCache.frontmatter;
// @expect-no-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
frontmatter;

// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app.metadataCache.getFileCache.frontmatter;

// Static computed properties retain the rooted chain.
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
app['metadataCache']['getFileCache']['frontmatter'];
import { parseFrontMatterAliases, parseFrontMatterTags } from "obsidian";
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
parseFrontMatterAliases(frontmatter);
// @expect-error glass-lint rule=obsidian:metadata.frontmatter-read message_id=detected
parseFrontMatterTags(frontmatter);
