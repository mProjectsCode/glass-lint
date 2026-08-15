use super::*;

#[test]
fn namespace_imports_follow_star_reexports() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_module("web", "request"))
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
    let rule = Rule::catalog_builder("network.request")
        .description("Uses request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_module("web", "request"))
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
    let rule = Rule::catalog_builder("network.request")
        .description("Uses request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_module("web", "request"))
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
    let rule = Rule::catalog_builder("network.request")
        .description("Uses request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(
            EventQuery::call_module("web", "request")
                .unwrap()
                .with_arg_static_string(0)
                .unwrap()
                .into_query(),
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
    let rule = Rule::catalog_builder("network.request")
        .description("Uses request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(
            EventQuery::call_module("web", "request")
                .unwrap()
                .with_arg_static_string(0)
                .unwrap()
                .into_query(),
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
