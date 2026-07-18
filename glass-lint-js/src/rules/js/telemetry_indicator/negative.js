// @case description negative fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import "@sentry/browser-extra";
// Similar module names do not establish telemetry-module provenance.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import unrelatedSentry from "@sentry/core";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import localAnalytics from "analytics.example";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import unrelatedDatadog from "@datadog/browser-rum-helper";

// Unconfigured domains are ignored.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const ordinaryAnalytics = "analytics.example.net";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const unrelatedCollector = "https://api.amplitude.example";

// Literal matching does not reconstruct concatenated or dynamic values.
const concatenated = "sent" + "ry.io";
const host = getHost();
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const dynamicEndpoint = "https://" + host;

// A local helper is unrelated to telemetry indicators.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator message_id=detected
function localLookalike() { return null; }
localLookalike();
