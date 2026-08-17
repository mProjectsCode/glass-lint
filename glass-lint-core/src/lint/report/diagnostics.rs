use crate::{
    analysis::ProjectSemanticModel,
    lint::report::{ProjectReportSession, files::ReportFiles},
    project::Diagnostic,
};

pub(super) fn attach_project_diagnostics(
    project: &ProjectSemanticModel,
    session: &ProjectReportSession,
    files: &mut ReportFiles,
) {
    let (status_files, status_project) = session.status_diagnostics();
    for (path, diagnostic) in status_files {
        files.push_file_diagnostic(&path, Diagnostic::project(diagnostic));
    }

    for diagnostic in project.diagnostics().iter().cloned() {
        if let Some(path) = diagnostic
            .location()
            .map(|location| location.path().clone())
        {
            files.push_file_diagnostic(&path, Diagnostic::project(diagnostic));
        } else {
            files.push_project_diagnostic(Diagnostic::project(diagnostic));
        }
    }
    for diagnostic in status_project {
        files.push_project_diagnostic(Diagnostic::project(diagnostic));
    }
}
