// @case description negative fixture for js:archive.compression
// @tool glass-lint rules=js:archive.compression
// @expect-no-error glass-lint rule=js:archive.compression message_id=detected
function localLookalike() { return null; }
localLookalike();
import unrelatedArchive from "archive-helper";
// @expect-no-error glass-lint rule=js:archive.compression message_id=detected
unrelatedArchive;
