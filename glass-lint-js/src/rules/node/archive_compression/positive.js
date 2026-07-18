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
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import archiver from "archiver";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import yauzl from "yauzl";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import unzipper from "unzipper";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import nodeTar from "node-tar";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import compressing from "compressing";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import admZip from "adm-zip";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import extractZip from "extract-zip";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import tarStream from "tar-stream";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import pako from "pako";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import decompress from "decompress";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import zipFolder from "zip-a-folder";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import zipJs from "@zip.js/zip.js";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import yazl from "yazl";
// @expect-error glass-lint rule=node:archive.compression message_id=detected
import streamZip from "node-stream-zip";
