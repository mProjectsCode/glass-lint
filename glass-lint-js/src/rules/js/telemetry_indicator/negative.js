// @case description negative fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
import "@sentry/browser-extra";
// Similar module names do not establish telemetry-module provenance.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
import unrelatedSentry from "@sentry/core";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
import localAnalytics from "analytics.example";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
import unrelatedDatadog from "@datadog/browser-rum-helper";

// Unconfigured domains are ignored.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
const ordinaryAnalytics = "analytics.example.net";
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
const unrelatedCollector = "https://api.amplitude.example";

// Literal matching does not reconstruct concatenated or dynamic values.
const concatenated = "sent" + "ry.io";
const host = getHost();
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
const dynamicEndpoint = "https://" + host;

// A local helper is unrelated to telemetry indicators.
// @expect-no-error glass-lint rule=js:network.telemetry-indicator
function localLookalike() { return null; }
localLookalike();
