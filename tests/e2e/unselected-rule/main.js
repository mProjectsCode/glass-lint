// @case description A selected rule does not report an unrelated capability
// @tool glass-lint rules=obsidian:vault.read

// Migrated: network/unselected-rule.js
fetch('/api/data');
