//! Case, adapter-protocol, result, and profiling data contracts.

mod case;
mod protocol;
mod report;

pub use case::{
    BundleKey, BundleProfile, BundleProfileError, BundleTarget, BundleTransformer, Case, CaseError,
    ExpectationError, ExpectedCount, FindingExpectation, FindingExpectationError, ProjectCase,
    ToolExpectation, normalize_bundle_profiles,
};
pub use protocol::{
    ADAPTER_PROTOCOL_VERSION, AdapterConversionError, AdapterFile, AdapterProject, AdapterRequest,
    AdapterResolution, AdapterResolutionKind, AdapterResolutionResult, AdapterResponse,
};
pub use report::{AdapterRun, BundleResult, CaseResult, SuiteReport, ToolResult};

#[cfg(test)]
mod tests;
