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
/// A staged project collection session. Sources are admitted deterministically,
/// then locally analyzed independently before `finish` links the retained
/// models after all resolver answers have been recorded.
pub use session::AnalysisSession;
pub use tables::EvidenceList;
pub use types::{
    AnalysisDiagnostic, AnalysisOperationCounts, AnalysisReport, AnalysisReportSummary, Diagnostic,
    DiagnosticCode, Evidence, FileReport, Finding, LinkedModuleTarget, ModuleId, ProjectInput,
    ProjectInputError, ProjectRelativePath, ReportCompletion, ResolutionRequest,
    ResolutionRequestKey, ResolutionRequestKind, ResolverOutcome, SourceFile, SourceLocation,
    SourceText, is_internal_module_request,
};

#[cfg(test)]
mod tests;
