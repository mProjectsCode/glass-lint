use crate::project::types::{Diagnostic, Finding, ProjectRelativePath};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileReport {
    path: ProjectRelativePath,
    findings: Vec<Finding>,
    diagnostics: Vec<Diagnostic>,
}

impl FileReport {
    pub fn new(
        path: ProjectRelativePath,
        findings: Vec<Finding>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            path,
            findings,
            diagnostics,
        }
    }

    pub fn path(&self) -> &ProjectRelativePath {
        &self.path
    }

    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub(crate) fn diagnostics_mut(&mut self) -> &mut Vec<Diagnostic> {
        &mut self.diagnostics
    }

    pub fn into_parts(self) -> (ProjectRelativePath, Vec<Finding>, Vec<Diagnostic>) {
        (self.path, self.findings, self.diagnostics)
    }

    #[must_use]
    pub fn has_parse_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::Parse { .. }))
    }

    #[must_use]
    pub fn parse_diagnostic_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::Parse { .. }))
            .count()
    }
}
