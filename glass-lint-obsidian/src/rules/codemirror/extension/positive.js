// @case description exact CodeMirror package loads are reported
// @tool glass-lint rules=obsidian:codemirror.extension
// Each configured package is covered through an ESM load.
// @expect-error glass-lint rule=obsidian:codemirror.extension
import { EditorState } from '@codemirror/state';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import { EditorView } from '@codemirror/view';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import * as language from '@codemirror/language';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import * as commands from '@codemirror/commands';

// An exact static CommonJS load retains module provenance too.
// @expect-error glass-lint rule=obsidian:codemirror.extension
const state = require('@codemirror/state');
// Common language, editor, and Lezer packages are covered by exact specifier.
// @expect-error glass-lint rule=obsidian:codemirror.extension
import markdown from '@codemirror/lang-markdown';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import javascript from '@codemirror/lang-javascript';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import json from '@codemirror/lang-json';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import autocomplete from '@codemirror/autocomplete';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lint from '@codemirror/lint';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import search from '@codemirror/search';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import collab from '@codemirror/collab';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lezerCommon from '@lezer/common';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lezerHighlight from '@lezer/highlight';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lezerLr from '@lezer/lr';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lezerJavascript from '@lezer/javascript';
// @expect-error glass-lint rule=obsidian:codemirror.extension
import lezerMarkdown from '@lezer/markdown';
