// @case description exact CodeMirror package loads are reported
// @tool glass-lint rules=obsidian:codemirror.extension
// Each configured package is covered through an ESM load.
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import { EditorState } from '@codemirror/state';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import { EditorView } from '@codemirror/view';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import * as language from '@codemirror/language';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import * as commands from '@codemirror/commands';

// An exact static CommonJS load retains module provenance too.
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
const state = require('@codemirror/state');
// Common language, editor, and Lezer packages are covered by exact specifier.
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import markdown from '@codemirror/lang-markdown';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import javascript from '@codemirror/lang-javascript';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import json from '@codemirror/lang-json';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import autocomplete from '@codemirror/autocomplete';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lint from '@codemirror/lint';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import search from '@codemirror/search';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import collab from '@codemirror/collab';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lezerCommon from '@lezer/common';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lezerHighlight from '@lezer/highlight';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lezerLr from '@lezer/lr';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lezerJavascript from '@lezer/javascript';
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import lezerMarkdown from '@lezer/markdown';
