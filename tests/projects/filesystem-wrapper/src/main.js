// @tool glass-lint rules=obsidian:network.request
import { send } from "./helper";

send(); // @expect-error glass-lint rule=obsidian:network.request line=4
