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
mod tests;
