// @case description configured renderer chains and static computed methods
// @tool glass-lint rules=obsidian:markdown.render
import * as obsidianApi from "obsidian";

// Module namespace provenance is recognized in addition to the legacy
// heuristic spellings.
// @expect-error glass-lint rule=obsidian:markdown.render
obsidianApi.MarkdownRenderer.render(app, text, el, '', ctx);

// Unproven bare receivers are intentionally excluded.
// @expect-no-error glass-lint rule=obsidian:markdown.render
MarkdownRenderer.render(app,text,el,'',ctx);
// @expect-no-error glass-lint rule=obsidian:markdown.render
MarkdownRenderer['render'](app, text, el, '', ctx);

// An unproven namespace-shaped global is intentionally excluded.
// @expect-no-error glass-lint rule=obsidian:markdown.render
obsidian.MarkdownRenderer.render(app, text, el, '', ctx);
// @expect-no-error glass-lint rule=obsidian:markdown.render
obsidian.MarkdownRenderer['render'](app, text, el, '', ctx);
