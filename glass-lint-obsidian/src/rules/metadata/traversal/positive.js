// @case description all Object traversal methods and rooted metadata maps
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.keys(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.entries(app.metadataCache.unresolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.values(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.keys(app.metadataCache.unresolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.entries(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.getOwnPropertyNames(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.getOwnPropertySymbols(app.metadataCache.unresolvedLinks);
// Descriptor and Reflect enumeration preserve the same rooted-map contract.
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.getOwnPropertyDescriptors(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
Reflect.ownKeys(app.metadataCache.unresolvedLinks);
// Configured Node/Electron global-object spellings are identity-safe too.
// @expect-error glass-lint rule=obsidian:metadata.traversal
global.Object.keys(app.metadataCache.resolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
global.Reflect.ownKeys(app.metadataCache.unresolvedLinks);
// @expect-error glass-lint rule=obsidian:metadata.traversal
globalThis.Object.keys(app.metadataCache.resolvedLinks);

const unresolved = app.metadataCache.unresolvedLinks;
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.values(unresolved);

const resolvedLinks = this.app.metadataCache.resolvedLinks;
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object.entries(resolvedLinks);

// Static computed Object members are accepted by the syntactic matcher.
// @expect-error glass-lint rule=obsidian:metadata.traversal
Object['keys'](resolvedLinks);
