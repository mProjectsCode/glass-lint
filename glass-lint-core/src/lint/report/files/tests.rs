use super::*;
use crate::project::types::{AnalysisDiagnostic, DiagnosticCode, DiagnosticKind};

#[test]
fn replacing_findings_preserves_file_diagnostics() {
    let path = ProjectRelativePath::new("main.js").unwrap();
    let diagnostic = Diagnostic::project(AnalysisDiagnostic::new(
        DiagnosticKind::FactCapacityExhausted.into(),
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
