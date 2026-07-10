// @case description positive fixture for obsidian:markdown.code-block-processor
// @tool glass-lint rules=obsidian:markdown.code-block-processor
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this.registerMarkdownCodeBlockProcessor('x',fn);
// second independent example

// @expect-error glass-lint rule=obsidian:markdown.code-block-processor message_id=detected
this.registerMarkdownCodeBlockProcessor("second", secondProcessor);
