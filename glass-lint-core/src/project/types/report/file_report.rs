use crate::project::types::{Diagnostic, Finding, ProjectRelativePath};

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileReport {
    path: ProjectRelativePath,
    findings: Vec<Finding>,
    diagnostics: Vec<Diagnostic>,
}

impl FileReport {
    pub(crate) fn new(
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

    pub(crate) fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn replace_findings(&mut self, findings: Vec<Finding>) {
        self.findings = findings;
    }

    #[must_use]
    pub fn has_parse_diagnostics(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_parse)
    }

    pub(crate) fn ordering_key(&self) -> &ProjectRelativePath {
        &self.path
    }

    #[must_use]
    pub fn parse_diagnostic_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_parse()).count()
    }
}
