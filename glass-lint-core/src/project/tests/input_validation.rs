use crate::{
    Position, SourceRange,
    project::{
        ProjectInputError, ResolutionRequestKey, ResolutionRequestKind, ResolverOutcome, tests::*,
    },
};

#[test]
fn staged_session_normalizes_and_sorts_sources() {
    let linter = test_linter();
    let mut collection = linter.begin_project("/project").unwrap();
    collection
        .analyze_source(source_file("./z.js", ""))
        .unwrap();
    collection.analyze_source(source_file("a.js", "")).unwrap();
    let report = finish_collection(collection);
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.files[0].path, "a.js");
    assert_eq!(report.files[1].path, "z.js");
}

#[test]
fn staged_session_rejects_duplicate_sources() {
    let linter = test_linter();
    let mut collection = linter.begin_project("/project").unwrap();
    assert!(collection.analyze_source(source_file("a.js", "")).is_ok());
    let result = collection.analyze_source(source_file("a.js", ""));
    assert!(result.is_err());
    assert!(matches!(result, Err(ProjectInputError::DuplicateSource(_))));
}

#[test]
fn staged_session_rejects_unknown_resolution_importers() {
    let linter = test_linter();
    let mut collection = linter.begin_project("/project").unwrap();
    assert!(collection.analyze_source(source_file("a.js", "")).is_ok());
    let result = collection.finish_local().resolve([(
        ResolutionRequestKey {
            importer: project_path("missing.js"),
            kind: ResolutionRequestKind::StaticImport,
            range: SourceRange::new(Position::new(1, 1).unwrap(), Position::new(1, 8).unwrap())
                .unwrap(),
        },
        ResolverOutcome::Missing,
    )]);
    assert!(result.is_err());
    assert!(matches!(result, Err(ProjectInputError::UnknownRequest(_))));
}
