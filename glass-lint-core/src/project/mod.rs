//! Validated, filesystem-free contracts for project-level analysis.
//!
//! The project API accepts owned sources and explicit resolver answers. It
//! normalizes paths and ranges, analyzes each source once, and preserves
//! module/file ownership when linking and reporting findings.

pub mod input;
mod report;
mod session;
pub mod tables;
pub mod types;
/// A staged project collection session. Sources are parsed and locally
/// analyzed when added; `finish` links the retained models after all resolver
/// answers have been recorded.
pub use session::ProjectSession;
pub use tables::{EvidenceList, ResolutionTable, SourceTable};
pub use types::{
    ModuleId, ProjectDiagnostic, ProjectEvidence, ProjectFileReport, ProjectFinding, ProjectInput,
    ProjectInputError, ProjectOperationCounts, ProjectReport, ProjectReportSummary,
    ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind, ResolutionResult,
    ResolvedModule, SourceFile, SourceLocation,
};

#[cfg(test)]
mod tests;
