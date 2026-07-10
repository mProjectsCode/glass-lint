// @case description positive fixture for obsidian:markdown.postprocessor
// @tool glass-lint rules=obsidian:markdown.postprocessor
// @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this.registerMarkdownPostProcessor(fn);
// second independent example

// @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this.registerMarkdownPostProcessor(secondProcessor);
