// @case description local, dynamic, reassigned, and unlisted traversal inputs
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
otherObject.keys(app.metadataCache.resolvedLinks);

const localLinks = {};
// @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.keys(localLinks);

let links = app.metadataCache.resolvedLinks;
links = localLinks;
// @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.entries(links);

function dynamicProperty(property) {
    // @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
    Object.keys(app.metadataCache[property]);
}

// @expect-no-error glass-lint rule=obsidian:metadata.traversal message_id=detected
Object.values(app.metadataCache.otherLinks);
