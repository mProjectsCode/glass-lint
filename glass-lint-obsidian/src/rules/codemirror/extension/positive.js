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
