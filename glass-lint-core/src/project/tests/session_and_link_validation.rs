use crate::project::tests::*;

#[test]
fn project_keeps_sorted_parse_failures_separate_from_valid_modules() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("z.js", "function {"))
        .unwrap();
    session
        .add_source(source_file("a.js", "fetch('/remote');"))
        .unwrap();
    let report = session.finish().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["a.js", "z.js"]
    );
    assert_eq!(report.files[0].findings.len(), 1);
    assert_eq!(report.files[1].findings.len(), 0);
    assert_eq!(report.files[1].parse_diagnostic_count(), 1);
}

#[test]
fn session_returns_static_import_dynamic_import_require_and_reexport_requests() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let requests = session
        .add_source(source_file(
            "main.js",
            "import { value as local } from './dep';\nexport { local as renamed } from './dep';\nconst x = require('./cjs');\nimport('./lazy');",
        ))
        .unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.key.kind)
            .collect::<Vec<_>>(),
        vec![
            ResolutionRequestKind::StaticImport,
            ResolutionRequestKind::StaticImport,
            ResolutionRequestKind::Require,
            ResolutionRequestKind::DynamicImport,
        ]
    );
    assert_eq!(requests[0].request, "./dep");
    assert_eq!(requests[2].request, "./cjs");
    assert_eq!(requests[3].request, "./lazy");
    assert_eq!(requests[2].key.range.start().column(), 19);
    assert_eq!(requests[2].key.range.end().column(), 26);
}

#[test]
fn session_rejects_resolution_for_an_unauthored_request() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.record_resolution(key("main.js"), ResolverOutcome::Missing);
    assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
}

#[test]
fn rejected_duplicate_source_does_not_replace_the_original() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.add_source(source_file("./main.js", ""));
    assert!(matches!(error, Err(ProjectInputError::DuplicateSource(_))));
    let report = session.finish().unwrap();
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn type_only_reexports_do_not_create_runtime_requests() {
    let linter = test_linter();
    let mut session = linter.begin_analysis("/project").unwrap();
    let requests = session
        .add_source(source_file(
            "types.ts",
            "export { type Foo } from './dependency';",
        ))
        .unwrap();
    assert!(requests.is_empty());
}

#[test]
fn linker_accepts_named_reexports_and_reports_missing_exports() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add("dep.js", "export const value = 1;");
    project.add_resolved(
        "barrel.js",
        "export { value } from './dep';",
        [ResolverOutcome::Internal {
            path: project_path("dep.js"),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolverOutcome::Internal {
            path: project_path("barrel.js"),
        }],
    );
    let report = project.finish();
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let mut missing = ProjectFixture::new(&linter);
    missing.add_resolved(
        "main.js",
        "import { nope } from './dep';",
        [ResolverOutcome::Internal {
            path: project_path("dep.js"),
        }],
    );
    missing.add("dep.js", "export const value = 1;");
    let report = missing.finish();
    assert!(report.files.iter().any(|file| {
        file.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == "missing_imported_export")
    }));
}

#[test]
fn linker_reports_ambiguous_multiple_star_exports() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add("a.js", "export const value = 1;");
    project.add("b.js", "export const value = 2;");
    project.add_resolved(
        "barrel.js",
        "export * from './a'; export * from './b';",
        [
            ResolverOutcome::Internal {
                path: project_path("a.js"),
            },
            ResolverOutcome::Internal {
                path: project_path("b.js"),
            },
        ],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolverOutcome::Internal {
            path: project_path("barrel.js"),
        }],
    );
    let report = project.finish();
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| diagnostic.code() == "ambiguous_star_export")
    );
}

#[test]
fn outside_project_targets_accept_normalized_absolute_paths() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import value from './outside';",
        [ResolverOutcome::OutsideProject {
            path: "/other/./dependency.js".into(),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report.files[0].diagnostics[0].code(),
        "outside_project_target"
    );
}

#[test]
fn dynamic_commonjs_export_shapes_are_reported_and_fail_closed() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import { value } from './dependency';",
        [ResolverOutcome::Internal {
            path: project_path("dependency.js"),
        }],
    );
    project.add("dependency.js", "module.exports = { value: 1, ...extra };");
    let report = project.finish();
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| diagnostic.code() == "unsupported_commonjs_exports")
    );
}
