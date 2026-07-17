// @case description negative fixture for browser:browser.environment
// @tool glass-lint rules=browser:browser.environment
// @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
// Unlisted environment properties are ignored.
// @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
screen.colorDepth;

// Dynamic property names are outside this direct-chain heuristic.
function read(navigator, property) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    navigator[property];
}
