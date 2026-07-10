// @case description negative fixture for obsidian:codemirror.extension
// @tool glass-lint rules=obsidian:codemirror.extension
// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import otherEditor from "other-editor-package";
