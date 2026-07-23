// @case description positive fixture for js:network.telemetry-indicator
// @tool glass-lint rules=js:network.telemetry-indicator
// @expect-error glass-lint rule=js:network.telemetry-indicator
import "@sentry/browser/profiling";
// Every configured telemetry SDK module is an exact module-provenance match.
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryBrowser from "@sentry/browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryNode from "@sentry/node";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import posthog from "posthog-js";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import mixpanel from "mixpanel-browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryElectron from "@sentry/electron";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryReact from "@sentry/react";
// More framework, browser, and exporter packages retain exact provenance.
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryVue from "@sentry/vue";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import sentryNext from "@sentry/nextjs";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import otel from "@opentelemetry/api";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import otelWeb from "@opentelemetry/sdk-trace-web";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import otlpExporter from "@opentelemetry/exporter-trace-otlp-http";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import segment from "@segment/analytics-next";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import amplitude from "@amplitude/analytics-browser";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import datadog from "@datadog/browser-rum";
// @expect-error glass-lint rule=js:network.telemetry-indicator
import logrocket from "@logrocket/react";

// Both configured endpoint markers are detected in literals.
// @expect-error glass-lint rule=js:network.telemetry-indicator
const sentryEndpoint = "https://project.sentry.io/api";
// @expect-error glass-lint rule=js:network.telemetry-indicator
const posthogEndpoint = "https://app.posthog.com/capture";
// @expect-error glass-lint rule=js:network.telemetry-indicator
const segmentEndpoint = "https://api.segment.io/v1";
// @expect-error glass-lint rule=js:network.telemetry-indicator
const datadogEndpoint = "https://browser-intake-datadoghq.com/api/v2";
// @expect-error glass-lint rule=js:network.telemetry-indicator
const amplitudeEndpoint = "https://api.amplitude.com/2/httpapi";
// @expect-error glass-lint rule=js:network.telemetry-indicator
const logrocketEndpoint = "https://r.logrocket.com/ingest";
