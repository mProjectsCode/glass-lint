use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use glass_lint_harness::{CaseTimings, SuiteReport, comparison};
use tracing_subscriber::{
    Layer, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

pub(crate) fn begin(path: &Path, adapter_count: usize) {
    eprintln!(
        "Running {} adapter(s) on cases in {}...",
        adapter_count,
        path.display()
    );
    tracing_subscriber::registry()
        .with(ProgressLayer)
        .try_init()
        .ok();
}

pub(crate) fn write_report(
    report: &SuiteReport,
    case_timings: &[CaseTimings],
    suite_elapsed: Duration,
) -> Result<()> {
    let mut tool_totals: BTreeMap<String, Duration> = BTreeMap::new();
    for timings in case_timings {
        for (name, duration) in timings {
            *tool_totals.entry(name.clone()).or_default() += *duration;
        }
    }

    let tool_names = tool_totals.keys().map(String::as_str).collect::<Vec<_>>();
    eprintln!(
        "Compared {} case(s) across tool(s): {}",
        report.cases.len(),
        tool_names.join(", ")
    );
    for (name, total) in &tool_totals {
        eprintln!("  {name}: {total:.1?}");
    }
    eprintln!("  total: {suite_elapsed:.1?}");

    let report_dir = Path::new("reports");
    fs::create_dir_all(report_dir).with_context(|| format!("create {}", report_dir.display()))?;
    let report_path = report_dir.join("COMPARISON.md");
    fs::write(&report_path, comparison(report))
        .with_context(|| format!("write {}", report_path.display()))?;
    eprintln!("Comparison report written to {}", report_path.display());
    Ok(())
}

struct ProgressLayer;

impl<S> Layer<S> for ProgressLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = ProgressVisitor(None);
        event.record(&mut visitor);
        if let Some(line) = visitor.0 {
            eprintln!("{line}");
        }
    }
}

struct ProgressVisitor(Option<String>);

impl tracing::field::Visit for ProgressVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "progress" {
            self.0 = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
}
