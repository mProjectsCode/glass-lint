// @case description similar, dynamic, and shadowed module loads are excluded
// @tool glass-lint rules=obsidian:codemirror.extension
// Similar package names do not have CodeMirror module provenance.
// @expect-no-error glass-lint rule=obsidian:codemirror.extension
import otherEditor from '@codemirror/view-wrapper';
// @expect-no-error glass-lint rule=obsidian:codemirror.extension
import languageLike from '@codemirror/lang-markdown-helper';
// @expect-no-error glass-lint rule=obsidian:codemirror.extension
import lezerLike from '@lezer/javascript-helper';

// @expect-no-error glass-lint rule=obsidian:codemirror.extension
import unrelatedEditor from 'other-editor-package';

// Dynamic names cannot be proven to be one of the configured packages.
const packageName = '@codemirror/view';
// @expect-no-error glass-lint rule=obsidian:codemirror.extension
require(packageName);

// A local CommonJS loader is not module provenance.
function require(name) { return { name }; }
// @expect-no-error glass-lint rule=obsidian:codemirror.extension
require('@codemirror/state');
