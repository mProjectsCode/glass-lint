// @case description proven Bases view registration
// @tool glass-lint rules=obsidian:bases.register
import { Plugin } from "obsidian";

class TestPlugin extends Plugin {
    run() {
        // @expect-error glass-lint rule=obsidian:bases.register message_id=detected
        this.registerBasesView("view", factory);
    }
}
