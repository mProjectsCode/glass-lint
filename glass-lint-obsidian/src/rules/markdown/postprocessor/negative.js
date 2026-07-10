// @case description negative fixture for obsidian:markdown.postprocessor
// @tool glass-lint rules=obsidian:markdown.postprocessor
// @expect-no-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
function localLookalike() { return null; }
localLookalike();
