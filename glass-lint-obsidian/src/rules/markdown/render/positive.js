// @case description configured renderer chains and static computed methods
// @tool glass-lint rules=obsidian:markdown.render
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer.render(app,text,el,'',ctx);
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer['render'](app, text, el, '', ctx);

// The second configured chain is syntactic too.
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
obsidian.MarkdownRenderer.render(app, text, el, '', ctx);
// @expect-error glass-lint rule=obsidian:markdown.render message_id=detected
obsidian.MarkdownRenderer['render'](app, text, el, '', ctx);
