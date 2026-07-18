// @case description local, dynamic, and alias registrations are excluded
// @tool glass-lint rules=obsidian:bases.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-no-error glass-lint rule=obsidian:bases.register message_id=detected
        this[method]("view", factory);
        const register = this.registerBasesView;
        // @expect-no-error glass-lint rule=obsidian:bases.register message_id=detected
        register("view", factory);
    }
}

const localPlugin = { registerBasesView() {} };
// @expect-no-error glass-lint rule=obsidian:bases.register message_id=detected
localPlugin.registerBasesView("view", factory);
