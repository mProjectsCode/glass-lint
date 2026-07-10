// @case description negative fixture for js:electron.module
// @tool glass-lint rules=js:electron.module
// @expect-no-error glass-lint rule=js:electron.module message_id=detected
function localLookalike() { return null; }
localLookalike();
