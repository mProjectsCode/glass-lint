// @case description negative fixture for obsidian:plugins.other-access
// @tool glass-lint rules=obsidian:plugins.other-access
// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.getOtherPlugin("id");
