use crate::{api::rule::EventQuery, project::tests::*};

#[test]
fn project_keeps_sorted_parse_failures_separate_from_valid_modules() {
    let linter = test_linter();
    let mut session = linter.begin_project().unwrap();
    session
        .analyze_source(source_file("z.js", "function {"))
        .unwrap();
    session
        .analyze_source(source_file("a.js", "fetch('/remote');"))
        .unwrap();
    let report = finish_collection(session);
    assert_eq!(
        report
            .files()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        ["a.js", "z.js"]
    );
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(report.files()[1].findings().len(), 0);
    assert_eq!(report.files()[1].parse_diagnostic_count(), 1);
}

#[test]
fn session_returns_static_import_dynamic_import_require_and_reexport_requests() {
    let linter = test_linter();
    let mut session = linter.begin_project().unwrap();
    let requests = session
        .analyze_source(source_file(
            "main.js",
            "import { value as local } from './dep';\nexport { local as renamed } from './dep';\nconst x = require('./cjs');\nimport('./lazy');",
        ))
        .unwrap()
        .requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .map(crate::project::ResolutionRequest::kind)
            .collect::<Vec<_>>(),
        vec![
            ResolutionRequestKind::StaticImport,
            ResolutionRequestKind::StaticImport,
            ResolutionRequestKind::Require,
            ResolutionRequestKind::DynamicImport,
        ]
    );
    assert_eq!(requests[0].specifier(), "./dep");
    assert_eq!(requests[2].specifier(), "./cjs");
    assert_eq!(requests[3].specifier(), "./lazy");
    assert_eq!(requests[2].range().start().column(), 19);
    assert_eq!(requests[2].range().end().column(), 26);
}

#[test]
fn session_rejects_resolution_for_an_unauthored_request() {
    let linter = test_linter();
    let mut session = linter.begin_project().unwrap();
    session
        .analyze_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session
        .finish_local()
        .resolve([(key("main.js"), ResolverOutcome::Missing)]);
    assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
}

#[test]
fn rejected_duplicate_source_does_not_replace_the_original() {
    let linter = test_linter();
    let mut session = linter.begin_project().unwrap();
    session
        .analyze_source(source_file("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.analyze_source(source_file("./main.js", ""));
    assert!(matches!(error, Err(ProjectInputError::DuplicateSource(_))));
    let report = finish_collection(session);
    assert_eq!(report.files()[0].findings().len(), 1);
}

#[test]
fn type_only_reexports_do_not_create_runtime_requests() {
    let linter = test_linter();
    let mut session = linter.begin_project().unwrap();
    let requests = session
        .analyze_source(source_file(
            "types.ts",
            "export { type Foo } from './dependency';",
        ))
        .unwrap()
        .requests();
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
        report.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics()
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
    assert!(report.files().iter().any(|file| {
        file.diagnostics()
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
            .files()
            .iter()
            .flat_map(crate::project::FileReport::diagnostics)
            .any(|diagnostic| diagnostic.code() == "ambiguous_star_export")
    );
}

#[test]
fn deep_namespace_export_chain_masks_unresolved_members() {
    let rule = Rule::builder("namespace.request")
        .description("Uses a deeply re-exported request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_module("./mod_0.js", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();
    let mut project = ProjectFixture::new(&linter);
    let depth = 1_024;

    project.add("mod_1024.js", "export const request = 1;");
    for index in (0..depth).rev() {
        let path = format!("mod_{index}.js");
        let next = format!("mod_{}.js", index + 1);
        project.add_resolved(
            &path,
            &format!("export * from './{next}';"),
            [ResolverOutcome::Internal {
                path: project_path(&next),
            }],
        );
    }
    project.add_resolved(
        "main.js",
        "import * as api from './mod_0.js'; api.request();",
        [ResolverOutcome::Internal {
            path: project_path("mod_0.js"),
        }],
    );

    let report = project.finish();
    let main = report
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .expect("main report");
    assert!(main.findings().is_empty());
}

#[test]
fn outside_project_targets_accept_normalized_absolute_paths() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import value from './outside';",
        [ResolverOutcome::OutsideProject {
            path: NormalizedOutsidePath::new("/other/dependency.js").unwrap(),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report.files()[0].diagnostics()[0].code(),
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
            .files()
            .iter()
            .flat_map(crate::project::FileReport::diagnostics)
            .any(|diagnostic| diagnostic.code() == "unsupported_commonjs_exports")
    );
}
