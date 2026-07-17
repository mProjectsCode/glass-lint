// @case description positive fixture for node:archive.compression
// @tool glass-lint rules=node:archive.compression

// Each configured package is reported at its import.
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import JSZip from "jszip";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import tar from "tar";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import zlib from "zlib";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import nodeZlib from "node:zlib";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import { gzipSync } from "fflate";
