use super::*;

fn file() -> ProjectRelativePath {
    ProjectRelativePath::new("main.js").unwrap()
}

#[test]
fn status_diagnostics_are_deduplicated_and_stable() {
    let mut status = AnalysisStatus::default();
    let reason = IncompleteReason::BudgetExhausted {
        component: AnalysisComponent::Effects,
        limit: 2,
        observed: Some(2),
    };
    status.record(StatusScope::File(file()), reason.clone());
    status.record(StatusScope::File(file()), reason);
    let (files, project) = status.diagnostics();
    assert_eq!(files.len(), 1);
    assert_eq!(project.len(), 0);
    assert_eq!(files[0].1.code().as_str(), "effect_size_budget_exhausted");
    assert!(files[0].1.message().contains("limit=2"));
}

#[test]
fn completion_depends_only_on_status_entries() {
    let mut status = AnalysisStatus::default();
    assert!(status.is_complete());
    status.record(
        StatusScope::Project,
        IncompleteReason::MissingInternalResolution {
            request: "./dep.js".into(),
        },
    );
    assert!(!status.is_complete());
}

#[test]
fn evidence_capacity_mismatch_has_a_project_diagnostic() {
    let mut status = AnalysisStatus::default();
    status.record(
        StatusScope::Project,
        IncompleteReason::EvidenceCapacityMismatch {
            expected: 2,
            actual: 3,
        },
    );

    let (files, project) = status.diagnostics();
    assert_eq!(files.len(), 0);
    assert_eq!(project.len(), 1);
    assert_eq!(project[0].code().as_str(), "evidence_capacity_mismatch");
    assert!(project[0].message().contains("expected=2, actual=3"));
}

#[test]
fn local_file_materialization_preserves_other_scopes() {
    let mut local = LocalAnalysisStatus::default();
    let reason = IncompleteReason::PathCapacityExhausted;
    local.record(reason.clone());

    let mut status = AnalysisStatus::default();
    status.record(StatusScope::File(file()), reason);

    let mut converted = local.materialize_file(&ProjectRelativePath::new("other.js").unwrap());
    converted.extend(&status);
    let (files, project) = converted.diagnostics();

    assert_eq!(project.len(), 0);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].0.as_str(), "main.js");
    assert_eq!(files[1].0.as_str(), "other.js");
}
