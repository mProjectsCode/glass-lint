//! Validated, filesystem-free contracts for project-level analysis.

mod input;
mod report;
mod session;
mod tables;
mod types;
/// A staged project collection session. Sources are parsed and locally
/// analyzed when added; `finish` links the retained models after all resolver
/// answers have been recorded.
pub use session::ProjectSession;
pub use tables::EvidenceList;
pub(crate) use tables::{ResolutionTable, SourceTable};
pub(crate) use types::{ModuleId, ResolvedModule};
pub use types::{
    ProjectDiagnostic, ProjectEvidence, ProjectFileReport, ProjectFinding, ProjectInput,
    ProjectInputError, ProjectOperationCounts, ProjectReport, ProjectReportSummary,
    ResolutionRequest, ResolutionRequestKey, ResolutionRequestKind, ResolutionResult, SourceFile,
    SourceLocation,
};

#[cfg(test)]
mod tests;
