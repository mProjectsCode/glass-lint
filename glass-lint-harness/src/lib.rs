mod adapters;
mod cases;
mod report;
mod runner;
mod types;

pub use adapters::{Adapter, ExternalAdapter, GlassLintAdapter};
pub use cases::load_cases;
pub use report::{failure_details, markdown, report_json, summary};
pub use runner::run_suite;
pub use types::{
    ADAPTER_PROTOCOL_VERSION, AdapterRequest, AdapterResponse, Case, CaseResult,
    DiagnosticExpectation, SuiteReport, ToolExpectation, ToolResult,
};
