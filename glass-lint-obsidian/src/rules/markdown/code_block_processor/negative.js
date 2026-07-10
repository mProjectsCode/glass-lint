// @case description negative fixture for obsidian:markdown.code-block-processor
// @tool glass-lint rules=obsidian:markdown.code-block-processor
// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this.registerMarkdownProcessor(handler);
