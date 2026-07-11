// @case description global and module-provenance Notice constructors/classes
// @tool glass-lint rules=obsidian:ui.notice
// The unbound spelling is a deliberate medium-confidence heuristic.
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new Notice("global");

import { Notice as ImportedNotice } from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new ImportedNotice("named import");
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
class ChildNotice extends ImportedNotice {}

// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new noticeNamespace.Notice("namespace");
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new CommonJsNotice("commonjs");

import * as noticeNamespace from "obsidian";
const { Notice: CommonJsNotice } = require("obsidian");
