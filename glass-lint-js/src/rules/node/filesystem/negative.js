// @case description negative fixture for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// Similar module names are not filesystem modules.
// @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
import localFs from "not-fs";
// @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
import pathLike from "node:path-browserify";

// @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
localFs;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
    require("fs");
    // @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
    require("node:path");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.filesystem message_id=detected
require(moduleName);
