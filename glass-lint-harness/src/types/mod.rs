//! Case, adapter-protocol, result, and profiling data contracts.

mod case;
mod protocol;
mod report;

pub(crate) use case::ToolSelector;
pub use case::{
    Case, CaseError, ExpectationError, ExpectedCount, FindingExpectation, FindingExpectationError,
    ProjectCase, ToolExpectation,
};
pub use protocol::{
    ADAPTER_PROTOCOL_VERSION, AdapterConversionError, AdapterFile, AdapterProject, AdapterRequest,
    AdapterResolution, AdapterResolutionKind, AdapterResolutionResult, AdapterResponse,
};
pub use report::{AdapterRun, CaseResult, SuiteReport, ToolResult};

#[cfg(test)]
mod tests;
