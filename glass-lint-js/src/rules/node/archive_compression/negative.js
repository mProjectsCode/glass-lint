// @case description negative fixture for js:archive.compression
// @tool glass-lint rules=js:archive.compression

// Similar package names are not treated as configured modules.
import unrelatedArchive from "archive-helper";
// @expect-no-error glass-lint rule=js:archive.compression message_id=detected
unrelatedArchive;

// A shadowed CommonJS loader is not module provenance.
function require(name) { return { name }; }
// @expect-no-error glass-lint rule=js:archive.compression message_id=detected
require("zlib");
