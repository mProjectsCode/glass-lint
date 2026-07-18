// @case description local, dynamic, and alias registrations are excluded
// @tool glass-lint rules=obsidian:cli.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-no-error glass-lint rule=obsidian:cli.register message_id=detected
        this[method]("command", handler);
        const register = this.registerCliHandler;
        // @expect-no-error glass-lint rule=obsidian:cli.register message_id=detected
        register("command", handler);
    }
}

const localPlugin = { registerCliHandler() {} };
// @expect-no-error glass-lint rule=obsidian:cli.register message_id=detected
localPlugin.registerCliHandler("command", handler);
