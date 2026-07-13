// @case description positive fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// Every configured telemetry SDK module is an exact module-provenance match.
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryBrowser from "@sentry/browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryNode from "@sentry/node";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import posthog from "posthog-js";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import mixpanel from "mixpanel-browser";

// Both configured endpoint markers are detected in literals.
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const sentryEndpoint = "https://project.sentry.io/api";
