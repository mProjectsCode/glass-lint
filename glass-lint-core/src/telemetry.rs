use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

#[derive(Clone, Copy, Debug)]
pub enum TelemetryLevel {
    Quiet,
    Normal,
    Verbose,
    Trace,
}
impl TelemetryLevel {
    fn filter(self) -> &'static str {
        match self {
            Self::Quiet => "warn",
            Self::Normal => "info",
            Self::Verbose => "debug",
            Self::Trace => "trace",
        }
    }
}

pub fn stderr_layer<S>(level: TelemetryLevel) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    stderr_layer_with_color(level, false)
}

pub fn stderr_layer_with_color<S>(level: TelemetryLevel, color: bool) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_ansi(color)
        .without_time()
        .with_span_events(
            if matches!(level, TelemetryLevel::Verbose | TelemetryLevel::Trace) {
                tracing_subscriber::fmt::format::FmtSpan::CLOSE
            } else {
                tracing_subscriber::fmt::format::FmtSpan::NONE
            },
        )
}

/// Build the shared formatter with an explicitly supplied output writer.
/// Front ends use this to keep telemetry off their result stream, while tests
/// and embedders can capture it without changing global process state.
pub fn layer_with_writer<S, W>(level: TelemetryLevel, writer: W) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    layer_with_writer_and_color(level, false, writer)
}

pub fn layer_with_writer_and_color<S, W>(
    level: TelemetryLevel,
    color: bool,
    writer: W,
) -> impl Layer<S> + Send + Sync
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(true)
        .with_ansi(color)
        .without_time()
        .with_span_events(
            if matches!(level, TelemetryLevel::Verbose | TelemetryLevel::Trace) {
                tracing_subscriber::fmt::format::FmtSpan::CLOSE
            } else {
                tracing_subscriber::fmt::format::FmtSpan::NONE
            },
        )
}

pub fn try_init(level: TelemetryLevel) -> Result<(), tracing_subscriber::util::TryInitError> {
    try_init_with_color(level, false)
}

pub fn try_init_with_color(
    level: TelemetryLevel,
    color: bool,
) -> Result<(), tracing_subscriber::util::TryInitError> {
    try_init_with_writer_and_color(level, color, std::io::stderr)
}

/// Install the shared formatter and filter using an explicit output writer.
pub fn try_init_with_writer<W>(
    level: TelemetryLevel,
    writer: W,
) -> Result<(), tracing_subscriber::util::TryInitError>
where
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    try_init_with_writer_and_color(level, false, writer)
}

pub fn try_init_with_writer_and_color<W>(
    level: TelemetryLevel,
    color: bool,
    writer: W,
) -> Result<(), tracing_subscriber::util::TryInitError>
where
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    Registry::default()
        .with(EnvFilter::new(level.filter()))
        .with(layer_with_writer_and_color(level, color, writer))
        .try_init()
}
