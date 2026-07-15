// @tool glass-lint rules=obsidian:network.request
import { send } from "./barrel";

send(); // @expect-error glass-lint rule=obsidian:network.request line=4

function local(send) {
  send(); // @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
}
