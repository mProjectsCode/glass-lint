// @case description negative fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const ordinaryAnalytics = "analytics.example.net";
