// @case description proven CLI handler registration
// @tool glass-lint rules=obsidian:cli.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-error glass-lint rule=obsidian:cli.register
        this.registerCliHandler("command", handler);
    }
}
