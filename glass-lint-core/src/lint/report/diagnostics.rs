use std::collections::BTreeMap;

use glass_lint_datastructures::{Position, SourceRange};

use crate::{
    ParseDiagnostic,
    analysis::ProjectSemanticModel,
    parse::ParseFailureKind,
    project::{Diagnostic, FileReport, ProjectRelativePath, SourceLocation, SourceTable},
};

pub(super) fn initialize_project_files(
    sources: &SourceTable,
    mut parse_diagnostics: BTreeMap<ProjectRelativePath, ParseDiagnostic>,
) -> (
    BTreeMap<ProjectRelativePath, FileReport>,
    BTreeMap<ProjectRelativePath, ParseFailureKind>,
) {
    let mut files = BTreeMap::new();
    let mut parse_failures = BTreeMap::new();
    for (path, source) in sources.iter() {
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
    (files, parse_failures)
}

pub(super) fn attach_project_diagnostics(
    project: &ProjectSemanticModel,
    files: &mut BTreeMap<ProjectRelativePath, FileReport>,
) -> Vec<Diagnostic> {
    let (status_files, status_project) = project.status_diagnostics();
    for (path, mut diagnostic) in status_files {
        diagnostic.set_location(Some(SourceLocation::new(
            path.clone(),
            SourceRange::new(
                Position::new(1, 1).expect("one-based position"),
                Position::new(1, 1).expect("one-based position"),
            )
            .expect("ordered source range"),
        )));
        if let Some(file) = files.get_mut(&path) {
            file.push_diagnostic(Diagnostic::project(diagnostic));
        }
    }

    let mut diagnostics = Vec::new();
    for diagnostic in project.diagnostics().iter().cloned() {
        if let Some(path) = diagnostic
            .location()
            .map(|location| location.path().clone())
        {
            if let Some(file) = files.get_mut(&path) {
                file.push_diagnostic(Diagnostic::project(diagnostic));
            }
        } else {
            diagnostics.push(Diagnostic::project(diagnostic));
        }
    }
    diagnostics.extend(status_project.into_iter().map(Diagnostic::project));
    diagnostics.sort_by(|left, right| left.code().cmp(right.code()));
    diagnostics
}
