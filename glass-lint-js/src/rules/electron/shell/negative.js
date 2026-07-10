// @case description negative fixture for js:electron.shell
// @tool glass-lint rules=js:electron.shell
// @expect-no-error glass-lint rule=js:electron.shell message_id=detected
function localLookalike() { return null; }
localLookalike();
const electron = { shell: { openExternal() {} } };
// @expect-no-error glass-lint rule=js:electron.shell message_id=detected
electron.shell.openExternal("local");
