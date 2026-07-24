use crate::{
    api::rule::{Category, MatcherDecl},
    project::tests::*,
};

#[test]
#[allow(clippy::too_many_lines)]
fn linked_internal_aliases_preserve_external_and_global_call_identity() {
    let external_rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    let global_rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![external_rule, global_rule]).unwrap()],
        environment,
    ))
    .unwrap();

    let mut session = linter.begin_project("/project").unwrap();
    let helper = session
        .analyze_source(source_file(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
        ))
        .unwrap()
        .requests();
    let main = session
        .analyze_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap()
        .requests();
    let report = finish_collection_with(
        session,
        [
            (
                helper[0].key.clone(),
                ResolverOutcome::External {
                    package: PackageSpecifier::new("web").unwrap(),
                },
            ),
            (
                main[0].key.clone(),
                ResolverOutcome::Internal {
                    path: project_path("helper.js"),
                },
            ),
        ],
    );
    let main_report = report
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .unwrap();
    assert_eq!(main_report.findings().len(), 1);
    assert_eq!(
        main_report.findings()[0].rule_id().as_str(),
        "test:network.request"
    );

    let mut global = linter.begin_project("/project").unwrap();
    let helper = global
        .analyze_source(source_file("helper.js", "export { fetch as send };"))
        .unwrap();
    let main = global
        .analyze_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap();
    let helper = helper.requests();
    let main = main.requests();
    assert!(helper.is_empty());
    let report = finish_collection_with(
        global,
        [(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )],
    );
    let main_report = report
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .unwrap();
    assert_eq!(main_report.findings().len(), 1);
    assert_eq!(
        main_report.findings()[0].rule_id().as_str(),
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
        .files()
        .iter()
        .find(|file| file.path().as_str() == "helper.js")
        .expect("helper report");
    assert_eq!(main.findings().len(), 1);
    assert_eq!(main.findings()[0].location().path().as_str(), "helper.js");
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
    assert!(
        report
            .files()
            .iter()
            .any(|file| !file.findings().is_empty())
    );
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
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .unwrap();
    assert_eq!(main.findings().len(), 1);
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
    assert!(report.files().iter().all(|file| file.findings().is_empty()));
}

#[test]
fn linked_unknown_exports_and_importer_reassignment_fail_closed() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut session = linter.begin_project("/project").unwrap();
    let helper = session
        .analyze_source(source_file(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
        ))
        .unwrap()
        .requests();
    let main = session
        .analyze_source(source_file(
            "main.js",
            "import { send } from './helper'; send = local; send();",
        ))
        .unwrap()
        .requests();
    let report = finish_collection_with(
        session,
        [
            (
                helper[0].key.clone(),
                ResolverOutcome::External {
                    package: PackageSpecifier::new("web").unwrap(),
                },
            ),
            (
                main[0].key.clone(),
                ResolverOutcome::Internal {
                    path: project_path("helper.js"),
                },
            ),
        ],
    );
    assert_eq!(
        report
            .files()
            .iter()
            .find(|file| file.path().as_str() == "main.js")
            .unwrap()
            .findings()
            .len(),
        0
    );

    let mut missing = linter.begin_project("/project").unwrap();
    let main = missing
        .analyze_source(source_file(
            "main.js",
            "import { send } from './helper'; send();",
        ))
        .unwrap();
    missing
        .analyze_source(source_file("helper.js", "export const other = 1;"))
        .unwrap();
    let main = main.requests();
    let report = finish_collection_with(
        missing,
        [(
            main[0].key.clone(),
            ResolverOutcome::Internal {
                path: project_path("helper.js"),
            },
        )],
    );
    assert_eq!(
        report
            .files()
            .iter()
            .find(|file| file.path().as_str() == "main.js")
            .unwrap()
            .findings()
            .len(),
        0
    );
}

#[test]
fn unresolved_internal_imports_do_not_become_external_provenance() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("./helper", "request")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();

    let mut session = linter.begin_project("/project").unwrap();
    session
        .analyze_source(source_file(
            "main.js",
            "import { request } from './helper'; request();",
        ))
        .unwrap();
    let report = finish_collection(session);

    assert!(report.files().iter().all(|file| file.findings().is_empty()));
    assert!(
        report
            .files()
            .iter()
            .flat_map(crate::project::FileReport::diagnostics)
            .any(|diagnostic| diagnostic.code() == "unresolved_internal_request")
    );
}

#[test]
fn commonjs_export_aliases_preserve_external_provenance_across_modules() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .unwrap();
    assert_eq!(main_report.findings().len(), 1);
}

#[test]
fn namespace_imports_follow_star_reexports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
        .files()
        .iter()
        .find(|file| file.path().as_str() == "main.js")
        .unwrap();
    assert_eq!(main_report.findings().len(), 1);
}

#[test]
fn static_dynamic_imports_follow_namespace_exports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
            .files()
            .iter()
            .find(|file| file.path().as_str() == "main.js")
            .unwrap()
            .findings()
            .len(),
        1
    );
}

#[test]
fn anonymous_commonjs_functions_remain_callable_across_modules() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration"),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
            .files()
            .iter()
            .find(|file| file.path().as_str() == "helper.js")
            .unwrap()
            .findings()
            .len(),
        1
    );
}

#[test]
fn returned_callable_provenance_crosses_an_exported_function() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration")
                .with_arg_static_string(0),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
            .files()
            .iter()
            .find(|file| file.path().as_str() == "main.js")
            .unwrap()
            .findings()
            .len(),
        1
    );
}

#[test]
fn linked_external_call_arguments_are_projected_after_reexports() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category(Category::new("network").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .call_module("web", "request")
                .build()
                .expect("valid matcher declaration")
                .with_arg_static_string(0),
        )
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
            package: PackageSpecifier::new("web").unwrap(),
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
            .files()
            .iter()
            .find(|file| file.path().as_str() == "main.js")
            .unwrap()
            .findings()
            .len(),
        1
    );
}
