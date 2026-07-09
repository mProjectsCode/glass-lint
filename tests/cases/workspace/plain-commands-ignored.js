// @case description Plain commands and event-name strings do not report editor integration
// @tool glass-lint rules=obsidian:workspace.editor_commands

this.addCommand({ id: "plain", callback() {} });
const eventName = "file-menu";
