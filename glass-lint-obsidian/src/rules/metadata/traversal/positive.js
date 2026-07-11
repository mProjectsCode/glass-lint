// @case description all Object traversal methods and rooted metadata maps
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.keys(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(app.metadataCache.unresolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.values(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.keys(app.metadataCache.unresolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(app.metadataCache.resolvedLinks);

const unresolved = app.metadataCache.unresolvedLinks;
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.values(unresolved);

const resolvedLinks = this.app.metadataCache.resolvedLinks;
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(resolvedLinks);

// Static computed Object members are accepted by the syntactic matcher.
// @expect-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object['keys'](resolvedLinks);
