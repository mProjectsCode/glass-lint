// @case description negative fixture for js:electron.ipc
// @tool glass-lint rules=js:electron.ipc
// @expect-no-error glass-lint rule=js:electron.ipc message_id=detected
function localLookalike() { return null; }
localLookalike();
