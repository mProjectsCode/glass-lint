// @case description local, dynamic, and alias registrations are excluded
// @tool glass-lint rules=obsidian:cli.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-no-error glass-lint rule=obsidian:cli.register
        this[method]("command", handler);
        const register = this.registerCliHandler;
        // @expect-error glass-lint rule=obsidian:cli.register
        register("command", handler);
    }
}

const localPlugin = { registerCliHandler() {} };
// @expect-no-error glass-lint rule=obsidian:cli.register
localPlugin.registerCliHandler("command", handler);
