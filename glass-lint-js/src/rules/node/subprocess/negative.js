// @case description negative fixture for node:node.subprocess
// @tool glass-lint rules=node:node.subprocess
// Similar module names are not Node subprocess modules.
// @expect-no-error glass-lint rule=node:node.subprocess
import localChildProcess from "not-child_process";
// @expect-no-error glass-lint rule=node:node.subprocess
import childProcessLike from "child_process-extra";
// @expect-no-error glass-lint rule=node:node.subprocess
import execaLike from "execa-extra";
// @expect-no-error glass-lint rule=node:node.subprocess
import spawnLike from "cross-spawn-extra";
// @expect-no-error glass-lint rule=node:node.subprocess
import localSpawn from "concurrently-helper";

// @expect-no-error glass-lint rule=node:node.subprocess
localChildProcess;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.subprocess
    require("child_process");
    // @expect-no-error glass-lint rule=node:node.subprocess
    require("node:child_process");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.subprocess
require(moduleName);
