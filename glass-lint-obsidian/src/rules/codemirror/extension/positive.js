// @case description positive fixture for obsidian:codemirror.extension
// @tool glass-lint rules=obsidian:codemirror.extension
// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import { EditorView } from '@codemirror/view';
// second independent example

// @expect-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import { EditorView as SecondEditorView } from "@codemirror/view";
