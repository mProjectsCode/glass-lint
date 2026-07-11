// @case description direct and statically-computed postprocessor registration
// @tool glass-lint rules=obsidian:markdown.postprocessor
// @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this.registerMarkdownPostProcessor(fn);
// @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
this['registerMarkdownPostProcessor'](secondProcessor);

// Receiver provenance is intentionally not established by this heuristic.
function unrelatedReceiver() {
    // @expect-error glass-lint rule=obsidian:markdown.postprocessor message_id=detected
    this.registerMarkdownPostProcessor(unrelatedProcessor);
}
