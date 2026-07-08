// @case description Ported old classifier case: string timer is dynamic code
// @tool glass-lint rules=obsidian:dynamic_code

setTimeout("runCode()", 0); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
window.setInterval(`runCode()`, 1000); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
