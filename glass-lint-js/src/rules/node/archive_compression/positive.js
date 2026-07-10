// @case description positive fixture for js:archive.compression
// @tool glass-lint rules=js:archive.compression

// Each configured package is reported at its import.
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import JSZip from "jszip";
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import tar from "tar";
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import zlib from "zlib";
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import nodeZlib from "node:zlib";
// @expect-error glass-lint rule=js:archive.compression message_id=detected
import { gzipSync } from "fflate";
