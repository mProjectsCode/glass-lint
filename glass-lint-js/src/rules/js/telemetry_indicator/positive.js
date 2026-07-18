// @case description positive fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import "@sentry/browser/profiling";
// Every configured telemetry SDK module is an exact module-provenance match.
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryBrowser from "@sentry/browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryNode from "@sentry/node";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import posthog from "posthog-js";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import mixpanel from "mixpanel-browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryElectron from "@sentry/electron";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryReact from "@sentry/react";
// More framework, browser, and exporter packages retain exact provenance.
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryVue from "@sentry/vue";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import sentryNext from "@sentry/nextjs";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import otel from "@opentelemetry/api";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import otelWeb from "@opentelemetry/sdk-trace-web";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import otlpExporter from "@opentelemetry/exporter-trace-otlp-http";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import segment from "@segment/analytics-next";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import amplitude from "@amplitude/analytics-browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import datadog from "@datadog/browser-rum";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
import logrocket from "@logrocket/react";

// Both configured endpoint markers are detected in literals.
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const sentryEndpoint = "https://project.sentry.io/api";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const posthogEndpoint = "https://app.posthog.com/capture";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const segmentEndpoint = "https://api.segment.io/v1";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const datadogEndpoint = "https://browser-intake-datadoghq.com/api/v2";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const amplitudeEndpoint = "https://api.amplitude.com/2/httpapi";
// @expect-error glass-lint rule=js:network.telemetry-indicator message_id=detected
const logrocketEndpoint = "https://r.logrocket.com/ingest";
