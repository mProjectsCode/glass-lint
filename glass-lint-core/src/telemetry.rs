//! Optional tracing subscriber construction for core/front-end diagnostics.

use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

#[derive(Clone, Copy, Debug)]
/// Coarse tracing filter level exposed to front ends.
pub enum TelemetryLevel {
    /// Warnings and errors only.
    Quiet,
    /// Informational events.
    Normal,
    /// Debug events.
    Verbose,
    /// Full trace events.
    Trace,
}

#[derive(Clone, Copy, Debug)]
/// Formatting and filtering choices for one telemetry installation.
pub struct TelemetryOptions {
    /// Minimum Glass Lint event level.
    pub level: TelemetryLevel,
    /// Whether the formatter emits ANSI color escapes.
    pub color: bool,
}

impl TelemetryOptions {
    pub const fn new(level: TelemetryLevel) -> Self {
        Self {
            level,
            color: false,
        }
    }

    #[must_use]
    pub const fn color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }
}

impl TelemetryLevel {
    /// Keep verbose output scoped to Glass Lint. Dependencies such as
    /// `oxc_resolver` have useful diagnostics, but their debug spans include
    /// implementation details (notably the complete `ResolveOptions`) that
    /// are not actionable at the CLI.
    fn filter(self) -> String {
        let level = match self {
            Self::Quiet => "warn",
            Self::Normal => "info",
            Self::Verbose => "debug",
            Self::Trace => "trace",
        };
        format!(
            "warn,glass_lint={level},glass_lint_core={level},glass_lint_project={level},glass_lint_cli={level},glass_lint_harness={level}"
        )
    }
}

/// Build a tracing layer that writes formatted events to stderr.
pub fn layer<S, W>(options: TelemetryOptions, writer: W) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(true)
        .with_ansi(options.color)
        .without_time()
        .with_span_events(if matches!(options.level, TelemetryLevel::Trace) {
            tracing_subscriber::fmt::format::FmtSpan::CLOSE
        } else {
            tracing_subscriber::fmt::format::FmtSpan::NONE
        })
}

/// Install one explicitly configured telemetry layer.
pub fn try_init<W>(
    options: TelemetryOptions,
    writer: W,
) -> Result<(), tracing_subscriber::util::TryInitError>
where
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    Registry::default()
        .with(EnvFilter::new(options.level.filter()))
        .with(layer(options, writer))
        .try_init()
}
