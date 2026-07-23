// @case description global and module-provenance Notice constructors/classes
// @tool glass-lint rules=obsidian:ui.notice
// The active window may be the main window, so it shares the configured globals.
// @expect-error glass-lint rule=obsidian:ui.notice
new activeWindow.Notice("active window");

// @expect-error glass-lint rule=obsidian:ui.notice
new globalThis.Notice("global object");
const GlobalNoticeAlias = window.Notice;
// @expect-error glass-lint rule=obsidian:ui.notice
new GlobalNoticeAlias("global alias");

// @expect-error glass-lint rule=obsidian:ui.notice
new Notice("global");

import { Notice as ImportedNotice } from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.notice
new ImportedNotice("named import");
// @expect-error glass-lint rule=obsidian:ui.notice
class ChildNotice extends ImportedNotice {}

// @expect-error glass-lint rule=obsidian:ui.notice
new noticeNamespace.Notice("namespace");
// @expect-error glass-lint rule=obsidian:ui.notice
new CommonJsNotice("commonjs");

import * as noticeNamespace from "obsidian";
const { Notice: CommonJsNotice } = require("obsidian");
