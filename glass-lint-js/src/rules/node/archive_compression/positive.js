// @case description positive fixture for js:archive.compression
// @tool glass-lint rules=js:archive.compression
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import z from "node:zlib";
// second independent example

// @expect-error glass-lint rule=js:archive.compression message_id=detected
import * as secondZip from "jszip";
