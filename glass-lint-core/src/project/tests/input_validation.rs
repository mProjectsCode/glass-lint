use crate::project::tests::*;

#[test]
fn validation_normalizes_and_sorts_sources_and_edges() {
    let validated = ProjectInput {
        root: "/project".into(),
        sources: vec![source_file("./z.js", ""), source_file("a.js", "")],
        resolutions: vec![(
            key("./z.js"),
            ResolverOutcome::Internal {
                path: project_path("./a.js"),
            },
        )],
    }
    .validate()
    .unwrap();

    let paths: Vec<_> = validated.sources().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["a.js", "z.js"]);

    let res: Vec<_> = validated.resolutions().collect();
    assert_eq!(res[0].0.importer.as_str(), "z.js");
    assert_eq!(
        res[0].1,
        &ResolverOutcome::Internal {
            path: project_path("a.js")
        }
    );

    assert_eq!(
        validated.module_id(&project_path("a.js")),
        Some(ModuleId::new(0))
    );
    assert_eq!(
        validated.module_id(&project_path("z.js")),
        Some(ModuleId::new(1))
    );
}

#[test]
fn duplicate_and_foreign_records_are_rejected() {
    let duplicate = ProjectInput {
        root: "/project".into(),
        sources: vec![source_file("a.js", ""), source_file("./a.js", "")],
        resolutions: vec![],
    }
    .validate();
    assert!(matches!(
        duplicate,
        Err(ProjectInputError::DuplicateSource(_))
    ));

    let foreign = ProjectInput {
        root: "/project".into(),
        sources: vec![source_file("a.js", "")],
        resolutions: vec![(key("missing.js"), ResolverOutcome::Missing)],
    }
    .validate();
    assert!(matches!(
        foreign,
        Err(ProjectInputError::UnknownImporter(_))
    ));
}
