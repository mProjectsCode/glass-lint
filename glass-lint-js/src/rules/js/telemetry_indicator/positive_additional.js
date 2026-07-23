// @case description additional literal coverage for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// The remaining configured endpoint marker is detected in a literal.
// @expect-error glass-lint rule=js:network.telemetry-indicator
const analyticsEndpoint = "https://www.google-analytics.com/collect";

// Static template fragments also provide literal evidence.
// @expect-error glass-lint rule=js:network.telemetry-indicator
const templatedEndpoint = `https://project.sentry.io/${resource}`;
