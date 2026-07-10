// @case description positive fixture for js:browser.environment
// @tool glass-lint rules=js:browser.environment
// @expect-error glass-lint rule=js:browser.environment message_id=detected
navigator.userAgent;
// Every configured property is a direct-read heuristic.
// @expect-error glass-lint rule=js:browser.environment message_id=detected
navigator.language;
// @expect-error glass-lint rule=js:browser.environment message_id=detected
screen.width;

// Deliberate heuristic gap: a shadowed local lookalike is reported too.
function inspect(navigator) {
    // @expect-error glass-lint rule=js:browser.environment message_id=detected
    navigator.userAgent;
}
