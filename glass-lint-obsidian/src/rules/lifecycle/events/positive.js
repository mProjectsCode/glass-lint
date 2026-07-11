// @case description all configured lifecycle registration chains
// @tool glass-lint rules=obsidian:lifecycle.events
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerEvent(app.vault.on('changed',fn));
// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerDomEvent(element, 'click', fn);
// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerInterval(setInterval(() => {}, 1000));

// Static computed names are canonicalized by the heuristic matcher.
// @expect-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this['registerEvent'](eventRef);

// Receiver provenance is intentionally not established.
function unrelatedReceiver() {
    // @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
    this.registerDomEvent(element, 'click', fn);
}
  }
}
