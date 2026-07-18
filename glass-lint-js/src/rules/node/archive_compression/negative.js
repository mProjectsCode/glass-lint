// @case description negative fixture for node:archive.compression
// @tool glass-lint rules=node:archive.compression

// Similar package names are not treated as configured modules.
import unrelatedArchive from "archive-helper";
// @expect-no-error glass-lint rule=node:archive.compression message_id=detected
import zipLike from "zip-a-folder-helper";
// @expect-no-error glass-lint rule=node:archive.compression message_id=detected
import zipJsLike from "@zip.js/zip-helper";
// @expect-no-error glass-lint rule=node:archive.compression message_id=detected
unrelatedArchive;

// A shadowed CommonJS loader is not module provenance.
function require(name) { return { name }; }
// @expect-no-error glass-lint rule=node:archive.compression message_id=detected
require("zlib");
