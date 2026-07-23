// @case description negative fixture for node:node.network
// @tool glass-lint rules=node:node.network
// Similar module names are not Node HTTP modules.
// @expect-no-error glass-lint rule=node:node.network
import localHttp from "not-http";
// @expect-no-error glass-lint rule=node:node.network
import http2Like from "http2-extra";
// @expect-no-error glass-lint rule=node:node.network
import wsLike from "ws-extra";
// @expect-no-error glass-lint rule=node:node.network
import dnsLike from "dns/promises-extra";
// @expect-no-error glass-lint rule=node:node.network
import localApollo from "@apollo/client-helper";

// @expect-no-error glass-lint rule=node:node.network
localHttp;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.network
    require("http");
    // @expect-no-error glass-lint rule=node:node.network
    require("node:https");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.network
require(moduleName);
