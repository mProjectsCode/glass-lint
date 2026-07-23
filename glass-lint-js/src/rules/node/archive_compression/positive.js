// @case description positive fixture for node:archive.compression
// @tool glass-lint rules=node:archive.compression

// Each configured package is reported at its import.
// @expect-error glass-lint rule=node:archive.compression
import JSZip from "jszip";
// @expect-error glass-lint rule=node:archive.compression
import tar from "tar";
// @expect-error glass-lint rule=node:archive.compression
import zlib from "zlib";
// @expect-error glass-lint rule=node:archive.compression
import nodeZlib from "node:zlib";
// @expect-error glass-lint rule=node:archive.compression
import { gzipSync } from "fflate";
// @expect-error glass-lint rule=node:archive.compression
import archiver from "archiver";
// @expect-error glass-lint rule=node:archive.compression
import yauzl from "yauzl";
// @expect-error glass-lint rule=node:archive.compression
import unzipper from "unzipper";
// @expect-error glass-lint rule=node:archive.compression
import nodeTar from "node-tar";
// @expect-error glass-lint rule=node:archive.compression
import compressing from "compressing";
// @expect-error glass-lint rule=node:archive.compression
import admZip from "adm-zip";
// @expect-error glass-lint rule=node:archive.compression
import extractZip from "extract-zip";
// @expect-error glass-lint rule=node:archive.compression
import tarStream from "tar-stream";
// @expect-error glass-lint rule=node:archive.compression
import pako from "pako";
// @expect-error glass-lint rule=node:archive.compression
import decompress from "decompress";
// @expect-error glass-lint rule=node:archive.compression
import zipFolder from "zip-a-folder";
// @expect-error glass-lint rule=node:archive.compression
import zipJs from "@zip.js/zip.js";
// @expect-error glass-lint rule=node:archive.compression
import yazl from "yazl";
// @expect-error glass-lint rule=node:archive.compression
import streamZip from "node-stream-zip";
