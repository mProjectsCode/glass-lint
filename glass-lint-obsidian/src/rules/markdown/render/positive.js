// @case description positive fixture for obsidian:markdown.render
// @tool glass-lint rules=obsidian:markdown.render
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer.render(app,text,el,'',ctx);
// second independent example
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer.render(app, text, el, "", ctx);
