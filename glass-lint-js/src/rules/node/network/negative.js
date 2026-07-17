// @case description negative fixture for node:node.network
// @tool glass-lint rules=node:node.network
// Similar module names are not Node HTTP modules.
// @expect-no-error glass-lint rule=node:node.network message_id=detected
import localHttp from "not-http";
// @expect-no-error glass-lint rule=node:node.network message_id=detected
import http2Like from "http2";

// @expect-no-error glass-lint rule=node:node.network message_id=detected
localHttp;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.network message_id=detected
    require("http");
    // @expect-no-error glass-lint rule=node:node.network message_id=detected
    require("node:https");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.network message_id=detected
require(moduleName);
