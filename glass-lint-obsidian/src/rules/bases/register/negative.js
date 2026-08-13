// @case description local and dynamic registrations are excluded; aliases retain identity
// @tool glass-lint rules=obsidian:bases.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-no-error glass-lint rule=obsidian:bases.register
        this[method]("view", factory);
        const register = this.registerBasesView;
        // @expect-error glass-lint rule=obsidian:bases.register
        register("view", factory);
    }
}

const localPlugin = { registerBasesView() {} };
// @expect-no-error glass-lint rule=obsidian:bases.register
localPlugin.registerBasesView("view", factory);
