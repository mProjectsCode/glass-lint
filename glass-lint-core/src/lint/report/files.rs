use std::collections::BTreeMap;

use crate::{
    ParseDiagnostic,
    parse::ParseFailureKind,
    project::{Diagnostic, FileReport, Finding, ProjectRelativePath, SourceTable},
};

pub(super) struct ReportFiles {
    files: BTreeMap<ProjectRelativePath, FileReport>,
    project_diagnostics: Vec<Diagnostic>,
}

impl ReportFiles {
    pub(super) fn initialize(
        sources: &SourceTable,
        mut parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
    ) -> (Self, BTreeMap<ProjectRelativePath, ParseFailureKind>) {
        let mut files = BTreeMap::new();
        let mut parse_failures = BTreeMap::new();
        for (path, source) in sources.in_normalized_path_order() {
            let path = path.clone();
            match parse_diagnostics.remove(&path) {
                Some(diagnostic) => {
                    parse_failures.insert(path.clone(), diagnostic.failure);
                    files.insert(
                        path,
                        FileReport::new(
                            source.path().clone(),
                            Vec::new(),
                            vec![Diagnostic::parse(source.path().clone(), diagnostic)],
                        ),
                    );
                }
                None => {
                    files.insert(path.clone(), FileReport::new(path, Vec::new(), Vec::new()));
                }
            }
        }
        for (path, diagnostic) in parse_diagnostics {
            parse_failures.insert(path, diagnostic.failure);
        }
        (
            Self {
                files,
                project_diagnostics: Vec::new(),
            },
            parse_failures,
        )
    }

    pub(super) fn replace_findings(&mut self, path: &ProjectRelativePath, findings: Vec<Finding>) {
        if let Some(file) = self.files.get_mut(path) {
            file.replace_findings(findings);
        } else {
            self.files.insert(
                path.clone(),
                FileReport::new(path.clone(), findings, Vec::new()),
            );
        }
    }

    pub(super) fn push_file_diagnostic(
        &mut self,
        path: &ProjectRelativePath,
        diagnostic: Diagnostic,
    ) {
        if let Some(file) = self.files.get_mut(path) {
            file.push_diagnostic(diagnostic);
        }
    }

    pub(super) fn push_project_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.project_diagnostics.push(diagnostic);
    }

    pub(super) fn into_parts(self) -> (Vec<FileReport>, Vec<Diagnostic>) {
        (self.files.into_values().collect(), self.project_diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::types::{AnalysisDiagnostic, DiagnosticCode, DiagnosticKind};

    #[test]
    fn replacing_findings_preserves_file_diagnostics() {
        let path = ProjectRelativePath::new("main.js").unwrap();
        let diagnostic = Diagnostic::project(AnalysisDiagnostic::new(
            DiagnosticKind::FactsBudgetExhausted.into(),
            "facts exhausted".into(),
            None,
        ));
        let mut files = ReportFiles {
            files: BTreeMap::from([(
                path.clone(),
                FileReport::new(path.clone(), Vec::new(), vec![diagnostic.clone()]),
            )]),
            project_diagnostics: Vec::new(),
        };

        files.replace_findings(&path, Vec::new());

        let (files, project_diagnostics) = files.into_parts();
        assert_eq!(files[0].diagnostics(), &[diagnostic]);
        assert_eq!(project_diagnostics.len(), 0);
    }

    #[test]
    fn project_diagnostics_are_kept_separate_from_files() {
        let mut files = ReportFiles {
            files: BTreeMap::new(),
            project_diagnostics: Vec::new(),
        };
        let diagnostic = Diagnostic::project(AnalysisDiagnostic::new(
            DiagnosticCode::new("project_issue").unwrap(),
            "project issue".into(),
            None,
        ));
        files.push_project_diagnostic(diagnostic.clone());

        let (files, project_diagnostics) = files.into_parts();
        assert_eq!(files.len(), 0);
        assert_eq!(project_diagnostics, vec![diagnostic]);
    }
}
