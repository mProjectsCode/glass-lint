// @case description Ported old classifier regression: plain commands and event-name strings should not be editor integration
// @tool glass-lint rules=obsidian:workspace.editor_commands

this.addCommand({ id: "plain", callback() {} });
const eventName = "file-menu";
