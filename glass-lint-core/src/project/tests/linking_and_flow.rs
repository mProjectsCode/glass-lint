use super::*;

#[test]
fn linked_internal_aliases_preserve_external_and_global_call_identity() {
    let external_rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let global_rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![external_rule, global_rule]).unwrap()],
        environment,
    ))
    .unwrap();

    let mut session = linter.begin_analysis("/project").unwrap();
    let helper = session
        .add_source(source_file(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
        ))
        .unwrap();
    let main = session
        .add_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap();
    session
        .record_resolution(
            helper[0].key.clone(),
            ResolverOutcome::External {
                package: "web".into(),
            },
        )
        .unwrap();
    session
        .record_resolution(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )
        .unwrap();
    let report = session.finish().unwrap();
    let main_report = report
        .files
        .iter()
        .find(|file| file.path == "main.js")
        .unwrap();
    assert_eq!(main_report.findings.len(), 1);
    assert_eq!(
        main_report.findings[0].rule_id.as_str(),
        "test:network.request"
    );

    let mut global = linter.begin_analysis("/project").unwrap();
    let helper = global
        .add_source(source_file("helper.js", "export { fetch as send };"))
        .unwrap();
    let main = global
        .add_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap();
    assert!(helper.is_empty());
    global
        .record_resolution(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )
        .unwrap();
    let report = global.finish().unwrap();
    let main_report = report
        .files
        .iter()
        .find(|file| file.path == "main.js")
        .unwrap();
    assert_eq!(main_report.findings.len(), 1);
    assert_eq!(
        main_report.findings[0].rule_id.as_str(),
        "test:network.fetch"
    );
}

#[test]
fn project_flow_crosses_an_exported_helper_boundary() {
    let linter = flow_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add(
        "helper.js",
        "export function append(element) { element.src = url; document.head.appendChild(element); }",
    );
    project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    let main = report
        .files
        .iter()
        .find(|file| file.path == "helper.js")
        .expect("helper report");
    assert_eq!(main.findings.len(), 1);
    assert_eq!(main.findings[0].location.path, "helper.js");
}

#[test]
fn project_flow_preserves_requirements_through_a_helper_chain() {
    let linter = flow_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add(
        "sink.js",
        "export function finish(element) { document.head.appendChild(element); }",
    );
    project.add_resolved(
        "helper.js",
        "import { finish } from './sink'; export function append(element) { element.src = url; finish(element); }",
        [ResolverOutcome::Internal {
            path: project_path("sink.js"),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    assert!(report.files.iter().any(|file| !file.findings.is_empty()));
}

#[test]
fn project_flow_follows_a_returned_parameter() {
    let linter = flow_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add(
        "helper.js",
        "export function identity(element) { return element; }",
    );
    project.add_resolved(
        "main.js",
        "import { identity } from './helper'; const element = document.createElement('script'); const returned = identity(element); returned.src = url; document.head.appendChild(returned);",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    let main = report
        .files
        .iter()
        .find(|file| file.path == "main.js")
        .unwrap();
    assert_eq!(main.findings.len(), 1);
}

#[test]
fn project_flow_fails_closed_for_unsupported_helper_control_flow() {
    let linter = flow_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add(
        "helper.js",
        "export function append(element) { if (ready) element.src = url; document.head.appendChild(element); }",
    );
    project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    assert!(report.files.iter().all(|file| file.findings.is_empty()));
}

#[test]
fn linked_unknown_exports_and_importer_reassignment_fail_closed() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut session = linter.begin_analysis("/project").unwrap();
    let helper = session
        .add_source(source_file(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
        ))
        .unwrap();
    let main = session
        .add_source(source_file(
            "main.js",
            "import { send } from './helper'; send = local; send();",
        ))
        .unwrap();
    session
        .record_resolution(
            helper[0].key.clone(),
            ResolverOutcome::External {
                package: "web".into(),
            },
        )
        .unwrap();
    session
        .record_resolution(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )
        .unwrap();
    let report = session.finish().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap()
            .findings
            .len(),
        0
    );

    let mut missing = linter.begin_analysis("/project").unwrap();
    let main = missing
        .add_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap();
    missing
        .add_source(source_file("helper.js", "export const other = 1;"))
        .unwrap();
    missing
        .record_resolution(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )
        .unwrap();
    let report = missing.finish().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap()
            .findings
            .len(),
        0
    );
}

#[test]
fn unresolved_internal_imports_do_not_become_external_provenance() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("./helper", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut session = linter.begin_analysis("/project").unwrap();
    session
        .add_source(source_file(
            "main.js",
            "import { request } from './helper'; request();",
        ))
        .unwrap();
    let report = session.finish().unwrap();

    assert!(report.files.iter().all(|file| file.findings.is_empty()));
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .any(|diagnostic| diagnostic.code() == "unresolved_internal_request")
    );
}

#[test]
fn commonjs_export_aliases_preserve_external_provenance_across_modules() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "const { request } = require('web'); exports.send = request;",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "const { send } = require('./helper'); send();",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();

    let main_report = report
        .files
        .iter()
        .find(|file| file.path == "main.js")
        .unwrap();
    assert_eq!(main_report.findings.len(), 1);
}

#[test]
fn namespace_imports_follow_star_reexports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "import { request } from 'web'; export { request };",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "barrel.js",
        "export * from './helper';",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    project.add_resolved(
        "main.js",
        "import * as api from './barrel'; api.request();",
        [ResolverOutcome::Internal {
            path: project_path("barrel.js"),
        }],
    );
    let report = project.finish();

    let main_report = report
        .files
        .iter()
        .find(|file| file.path == "main.js")
        .unwrap();
    assert_eq!(main_report.findings.len(), 1);
}

#[test]
fn static_dynamic_imports_follow_namespace_exports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "import { request } from 'web'; export { request };",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "async function run() { const api = await import('./helper'); api.request(); }",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap()
            .findings
            .len(),
        1
    );
}

#[test]
fn anonymous_commonjs_functions_remain_callable_across_modules() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("web", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "const { request } = require('web'); exports.send = () => request();",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "const { send } = require('./helper'); send();",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "helper.js")
            .unwrap()
            .findings
            .len(),
        1
    );
}

#[test]
fn returned_callable_provenance_crosses_an_exported_function() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::from(
            CallMatcher::module_export("web", "request").arg_static_string(0),
        ))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "import { request } from 'web'; export function get() { return request; }",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { get } from './helper'; get()('/remote');",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();
    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap()
            .findings
            .len(),
        1
    );
}

#[test]
fn linked_external_call_arguments_are_projected_after_reexports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::from(
            CallMatcher::module_export("web", "request").arg_static_string(0),
        ))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "helper.js",
        "import { request } from 'web'; export { request as send };",
        [ResolverOutcome::External {
            package: "web".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { send } from './helper'; send('/remote');",
        [ResolverOutcome::Internal {
            path: project_path("helper.js"),
        }],
    );
    let report = project.finish();

    assert_eq!(
        report
            .files
            .iter()
            .find(|file| file.path == "main.js")
            .unwrap()
            .findings
            .len(),
        1
    );
}

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
