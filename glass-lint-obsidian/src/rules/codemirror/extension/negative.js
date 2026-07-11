// @case description similar, dynamic, and shadowed module loads are excluded
// @tool glass-lint rules=obsidian:codemirror.extension
// Similar package names do not have CodeMirror module provenance.
// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import otherEditor from '@codemirror/view-wrapper';

// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
import unrelatedEditor from 'other-editor-package';

// Dynamic names cannot be proven to be one of the configured packages.
const packageName = '@codemirror/view';
// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
require(packageName);

// A local CommonJS loader is not module provenance.
function require(name) { return { name }; }
// @expect-no-error glass-lint rule=obsidian:codemirror.extension message_id=detected
require('@codemirror/state');
