use crate::project::tests::*;

#[test]
fn validation_normalizes_and_sorts_sources_and_edges() {
    let input = ProjectInput {
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

    assert_eq!(
        input
            .sources
            .iter()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>(),
        ["a.js", "z.js"]
    );
    assert_eq!(input.resolutions[0].0.importer, "z.js");
    assert_eq!(
        input.resolutions[0].1,
        ResolverOutcome::Internal {
            path: project_path("a.js")
        }
    );
    let validated = input.admit().unwrap();
    assert_eq!(validated.module_ids["a.js"], ModuleId::new(0));
    assert_eq!(validated.module_ids["z.js"], ModuleId::new(1));
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
