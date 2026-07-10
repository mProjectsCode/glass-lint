// @case description positive fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import x from "@sentry/browser";
// second independent example
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const telemetryEndpoint = "sentry.io";
