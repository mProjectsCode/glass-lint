// @case description negative fixture for obsidian:metadata.extract
// @tool glass-lint rules=obsidian:metadata.extract
// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
app.metadataCache.getFileCache.comments;
// Migrated: metadata/local-lookalikes-ignored.js and precision/capability-boundaries.js
const localMetadata = { tags: [], links: [], embeds: [] };

// @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected
localMetadata.tags;
