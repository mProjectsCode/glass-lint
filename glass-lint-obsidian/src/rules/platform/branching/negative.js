// @case description negative fixture for obsidian:platform.branching
// @tool glass-lint rules=obsidian:platform.branching
// @expect-no-error glass-lint rule=obsidian:platform.branching message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:platform.branching message_id=detected
const platformName = "unknown-platform";
