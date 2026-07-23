// @case description negative fixture for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// Similar module names are not filesystem modules.
// @expect-no-error glass-lint rule=node:node.filesystem
import localFs from "not-fs";
// @expect-no-error glass-lint rule=node:node.filesystem
import pathLike from "node:path-browserify";
// @expect-no-error glass-lint rule=node:node.filesystem
import extraLike from "fs-extra-tools";
// @expect-no-error glass-lint rule=node:node.filesystem
import memoryLike from "memfs-helper";
// @expect-no-error glass-lint rule=node:node.filesystem
import helper from "rimraf-helper";

// @expect-no-error glass-lint rule=node:node.filesystem
localFs;

// A shadowed CommonJS loader does not establish module provenance.
function shadowed(require) {
    // @expect-no-error glass-lint rule=node:node.filesystem
    require("fs");
    // @expect-no-error glass-lint rule=node:node.filesystem
    require("node:path");
}
shadowed(() => ({}));

// Dynamic module names are outside the static import matcher.
const moduleName = getModuleName();
// @expect-no-error glass-lint rule=node:node.filesystem
require(moduleName);
