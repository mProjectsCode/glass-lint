// @case description negative fixture for obsidian:metadata.cache-read
// @tool glass-lint rules=obsidian:metadata.cache-read
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:metadata.cache-read message_id=detected
app.otherCache.getFileCache(file);
