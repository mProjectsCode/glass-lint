// @case description positive fixture for js:browser.environment
// @tool glass-lint rules=js:browser.environment
// @expect-error glass-lint rule=js:browser.environment message_id=detected
navigator.userAgent;
// second independent example
// @expect-error glass-lint rule=js:browser.environment message_id=detected
navigator.language;
