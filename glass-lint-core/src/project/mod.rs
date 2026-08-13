//! Project-level data types, sessions, and report assembly.
//!
//! The project API accepts owned sources and explicit resolver answers. It
//! normalizes paths and ranges, analyzes each source once, and preserves
//! module/file ownership when linking and reporting findings. No filesystem
//! access happens in this crate; the project crate adds discovery and loading.

pub mod input;
mod report;
mod session;
mod tables;
pub mod types;
pub use report::ReportCombineError;
pub(crate) use session::SessionState;
pub use session::{AuthoredRequests, ProjectSession};
pub(crate) use tables::{ResolutionTable, SourceTable};
pub use types::{
    AnalysisDiagnostic, AnalysisOperationCounts, AnalysisReport, AnalysisReportSummary,
    BuiltinModuleName, Diagnostic, DiagnosticCode, EvidenceConstructionError, EvidenceRole,
    EvidenceStep, EvidenceTrace, EvidenceTraces, FileReport, Finding, LocalExecutionError,
    MatchCertainty, NormalizedOutsidePath, PackageSpecifier, ProjectError, ProjectExecutionError,
    ProjectInputError, ProjectPhaseError, ProjectRelativePath, ReportCompletion, ResolutionRequest,
    ResolutionRequestKey, ResolutionRequestKind, ResolverOutcome, SourceFile, SourceLocation,
    SourceText, is_internal_module_request,
};
pub(crate) use types::{LinkedModuleTarget, ModuleId};

#[cfg(test)]
mod tests;
