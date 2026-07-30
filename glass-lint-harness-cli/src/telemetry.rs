use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone, Copy, Debug)]
pub enum TelemetryLevel {
    Quiet,
}

#[derive(Clone, Copy, Debug)]
pub struct TelemetryOptions {
    level: TelemetryLevel,
    color: bool,
}

impl TelemetryOptions {
    pub(crate) const fn new(level: TelemetryLevel) -> Self {
        Self {
            level,
            color: false,
        }
    }

    pub(crate) const fn color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }
}

impl TelemetryLevel {
    fn filter(self) -> &'static str {
        match self {
            Self::Quiet => "warn",
        }
    }
}

pub fn try_init<W>(
    options: TelemetryOptions,
    writer: W,
) -> Result<(), tracing_subscriber::util::TryInitError>
where
    W: for<'writer> tracing_subscriber::fmt::writer::MakeWriter<'writer> + Send + Sync + 'static,
{
    Registry::default()
        .with(EnvFilter::new(options.level.filter()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_target(true)
                .with_ansi(options.color)
                .without_time(),
        )
        .try_init()
}
