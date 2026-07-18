// @case description negative fixture for node:node.subprocess
// @tool glass-lint rules=node:node.subprocess
// Similar module names are not Node subprocess modules.
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
import localChildProcess from "not-child_process";
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
import childProcessLike from "child_process-extra";
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
import execaLike from "execa-extra";
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
import spawnLike from "cross-spawn-extra";
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
import localSpawn from "concurrently-helper";

// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
localChildProcess;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
    require("child_process");
    // @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
    require("node:child_process");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.subprocess message_id=detected
require(moduleName);
