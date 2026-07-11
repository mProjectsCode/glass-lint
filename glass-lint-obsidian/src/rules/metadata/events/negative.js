// @case description shadowed, reassigned, dynamic, and unsupported events
// @tool glass-lint rules=obsidian:metadata.events
// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
otherCache.on('changed', handler);

function localApp(app) {
// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
    app.metadataCache.on('changed', handler);
}

let cache = app.metadataCache;
cache = otherCache;
// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
cache.on('resolved', handler);

const dynamicEvent = eventName;
// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
app.metadataCache.on(dynamicEvent, handler);

// @expect-no-error glass-lint rule=obsidian:metadata.events message_id=detected
app.metadataCache.on('renamed', handler);
