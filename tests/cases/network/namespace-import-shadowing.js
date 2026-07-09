// @case description An Obsidian namespace import is unavailable through a shadowing binding
// @tool glass-lint rules=obsidian:network.obsidian

import * as obsidian from "obsidian";

function localOnly(obsidian) {
  obsidian.requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.obsidian message_id=detected
}
