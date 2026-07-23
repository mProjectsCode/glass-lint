// @case description local, dynamic, reassigned, and unlisted traversal inputs
// @tool glass-lint rules=obsidian:metadata.traversal
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
otherObject.keys(app.metadataCache.resolvedLinks);

const localLinks = {};
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Object.keys(localLinks);

let links = app.metadataCache.resolvedLinks;
links = localLinks;
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Object.entries(links);

function dynamicProperty(property) {
    // @expect-no-error glass-lint rule=obsidian:metadata.traversal
    Object.keys(app.metadataCache[property]);
}

// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Object.values(app.metadataCache.otherLinks);
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Object.getOwnPropertyNames(app.metadataCache.otherLinks);
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Reflect.ownKeys(localLinks);
// @expect-no-error glass-lint rule=obsidian:metadata.traversal
Object.getOwnPropertyDescriptors(app.metadataCache.otherLinks);

function localGlobal(global) {
    // @expect-no-error glass-lint rule=obsidian:metadata.traversal
    global.Object.keys(app.metadataCache.resolvedLinks);
}
localGlobal({ Object: { keys() {} } });

function localObject(Object) {
    // @expect-no-error glass-lint rule=obsidian:metadata.traversal
    Object.keys(app.metadataCache.resolvedLinks);
}
