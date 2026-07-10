// @case description positive fixture for obsidian:metadata.traversal
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.keys(app.metadataCache.resolvedLinks);
// second independent example

// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(app.metadataCache.unresolvedLinks);
const unresolved = app.metadataCache.unresolvedLinks;

// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.values(unresolved);
// Migrated: metadata/flow-sensitive-metadata.js
const resolvedLinks = this.app.metadataCache.resolvedLinks;

// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(resolvedLinks);
