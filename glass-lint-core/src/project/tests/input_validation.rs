use glass_lint_datastructures::{Position, SourceRange};

use crate::project::{
    ProjectError, ProjectInputError, ProjectPhaseError, ResolutionRequestKey,
    ResolutionRequestKind, ResolverOutcome, tests::*,
};

#[test]
fn staged_session_normalizes_and_sorts_sources() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    collection
        .analyze_source(source_file("./z.js", ""))
        .unwrap();
    collection.analyze_source(source_file("a.js", "")).unwrap();
    let report = finish_session(collection);
    assert_eq!(report.files().len(), 2);
    assert_eq!(report.files()[0].path().as_str(), "a.js");
    assert_eq!(report.files()[1].path().as_str(), "z.js");
}

#[test]
fn staged_session_rejects_duplicate_sources() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    assert!(collection.analyze_source(source_file("a.js", "")).is_ok());
    let result = collection.analyze_source(source_file("a.js", ""));
    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(ProjectError::Input(ProjectInputError::DuplicateSource(_)))
    ));
}

#[test]
fn batch_source_admission_is_atomic_on_duplicate() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    collection
        .analyze_source(source_file("existing.js", ""))
        .unwrap();

    let result = collection.analyze_sources(
        [
            source_file("staged.js", ""),
            source_file("./existing.js", ""),
        ],
        std::num::NonZeroUsize::MIN,
    );
    assert!(matches!(
        result,
        Err(ProjectError::Input(ProjectInputError::DuplicateSource(_)))
    ));

    collection
        .analyze_source(source_file("staged.js", ""))
        .unwrap();
    let report = finish_session(collection);
    assert_eq!(report.files().len(), 2);
}

#[test]
fn source_admission_limits_are_atomic_and_report_typed_budget_errors() {
    let limits = crate::ProjectAdmissionLimits::new(3, 4).unwrap();
    let linter = test_linter_with_project_limits(limits);
    let mut collection = linter.begin_project();
    collection
        .analyze_source(source_file("first.js", ""))
        .unwrap();

    let result = collection.analyze_sources(
        [
            source_file("second.js", "12"),
            source_file("third.js", ""),
            source_file("fourth.js", ""),
        ],
        std::num::NonZeroUsize::MIN,
    );
    assert!(matches!(
        result,
        Err(ProjectError::Input(
            ProjectInputError::SourceCountExceeded { .. }
        ))
    ));
    collection
        .analyze_source(source_file("second.js", "12"))
        .unwrap();
    assert!(matches!(
        collection.analyze_source(source_file("too-large.js", "123")),
        Err(ProjectError::Input(
            ProjectInputError::SourceBytesExceeded { .. }
        ))
    ));
}

#[test]
fn staged_session_rejects_unknown_resolution_importers() {
    let linter = test_linter();
    let mut collection = linter.begin_project();
    assert!(collection.analyze_source(source_file("a.js", "")).is_ok());
    let result = collection.finish([(
        ResolutionRequestKey::new(
            project_path("missing.js"),
            ResolutionRequestKind::StaticImport,
            SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 8).unwrap()).unwrap(),
        ),
        ResolverOutcome::Missing,
    )]);
    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(ProjectError::Phase(ProjectPhaseError::UnknownRequest(_)))
    ));
}
