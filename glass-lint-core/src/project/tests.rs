//! Project-session integration tests for normalization, linking, flow, and
//! report ownership across multiple authored source files.

use super::*;
use crate::{
    Position, SourceRange,
    api::rule::{
        CallMatcher, Confidence, FlowCompletion, FlowCondition, FlowSinkMatcher, Matcher,
        MemberCallMatcher, ObjectEventMatcher, ObjectFlowMatcher, ObjectSourceMatcher, Rule,
        Severity, ValueMatcher,
    },
};

fn test_linter() -> crate::Linter {
    let rule = Rule::builder("network.fetch")
        .label("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    crate::Linter::new(
        crate::RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
    )
}

fn flow_linter() -> crate::Linter {
    let rule = Rule::builder("flow.append")
        .label("Appends a configured script")
        .category("flow")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(
            ObjectFlowMatcher::builder("script insertion")
                .source(ObjectSourceMatcher::returned_by(
                    MemberCallMatcher::rooted("document.createElement")
                        .arg(0, ValueMatcher::static_string().equals("script")),
                ))
                .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                    "src",
                    ValueMatcher::any_value(),
                )))
                .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                    MemberCallMatcher::rooted("document.head.appendChild"),
                    0,
                )]))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment
        .add_globals(["document", "url"])
        .expect("test environment globals");
    crate::Linter::new(
        crate::RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
    )
}

fn key(importer: &str) -> ResolutionRequestKey {
    ResolutionRequestKey {
        importer: importer.into(),
        kind: ResolutionRequestKind::Import,
        range: SourceRange {
            start: Position { line: 1, column: 1 },
            end: Position { line: 1, column: 8 },
        },
    }
}

struct ProjectFixture<'a> {
    session: ProjectSession<'a>,
}

impl<'a> ProjectFixture<'a> {
    fn new(linter: &'a crate::Linter) -> Self {
        Self {
            session: linter.begin_project("/project").unwrap(),
        }
    }

    fn add(&mut self, path: &str, source: &str) {
        self.session
            .add_source(SourceFile::new(path, source))
            .unwrap();
    }

    fn add_resolved(
        &mut self,
        path: &str,
        source: &str,
        resolutions: impl IntoIterator<Item = ResolutionResult>,
    ) {
        let requests = self
            .session
            .add_source(SourceFile::new(path, source))
            .unwrap();
        for (request, resolution) in requests.into_iter().zip(resolutions) {
            self.session
                .record_resolution(request.key, resolution)
                .unwrap();
        }
    }

    fn finish(self) -> ProjectReport {
        self.session.finish().unwrap()
    }
}

mod input_validation {
    use super::*;

    #[test]
    fn validation_normalizes_and_sorts_sources_and_edges() {
        let input = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("./z.js", ""), SourceFile::new("a.js", "")],
            resolutions: vec![(
                key("./z.js"),
                ResolutionResult::Internal {
                    path: "./a.js".into(),
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
            ResolutionResult::Internal {
                path: "a.js".into()
            }
        );
        assert_eq!(input.module_ids()["a.js"], ModuleId(0));
        assert_eq!(input.module_ids()["z.js"], ModuleId(1));
    }

    #[test]
    fn duplicate_and_foreign_records_are_rejected() {
        let duplicate = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", ""), SourceFile::new("./a.js", "")],
            resolutions: vec![],
        }
        .validate();
        assert!(matches!(
            duplicate,
            Err(ProjectInputError::DuplicateSource(_))
        ));

        let foreign = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", "")],
            resolutions: vec![(key("missing.js"), ResolutionResult::Missing)],
        }
        .validate();
        assert!(matches!(
            foreign,
            Err(ProjectInputError::UnknownImporter(_))
        ));

        let malformed_range = ProjectInput {
            root: "/project".into(),
            sources: vec![SourceFile::new("a.js", "")],
            resolutions: vec![(
                ResolutionRequestKey {
                    importer: "a.js".into(),
                    kind: ResolutionRequestKind::Import,
                    range: SourceRange {
                        start: Position { line: 1, column: 1 },
                        end: Position { line: 1, column: 0 },
                    },
                },
                ResolutionResult::Missing,
            )],
        }
        .validate();
        assert!(matches!(
            malformed_range,
            Err(ProjectInputError::InvalidRange(_))
        ));
    }
}

#[test]
fn session_uses_project_analysis_and_preserves_single_file_findings() {
    let linter = test_linter();
    let source = "fetch('/remote');\n";
    let direct = linter.lint(source, "a.js");

    let mut session = linter.begin_project("/project").unwrap();
    session.add_source(SourceFile::new("a.js", source)).unwrap();
    let project = session.finish().unwrap();

    assert_eq!(project.files.len(), 1);
    assert_eq!(project.files[0].path, "a.js");
    assert_eq!(project.files[0].findings.len(), direct.findings.len());
    assert_eq!(
        project.files[0].findings[0].location.range,
        direct.findings[0].range
    );
    assert_eq!(project.files[0].findings[0].location.path, "a.js");
    assert_eq!(
        project.files[0].findings[0].evidence[0]
            .location
            .as_ref()
            .map(|location| location.path.as_str()),
        Some("a.js")
    );
}

mod linking_and_flow {
    use super::*;

    #[test]
    fn linked_internal_aliases_preserve_external_and_global_call_identity() {
        let external_rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let global_rule = Rule::builder("network.fetch")
            .label("Uses fetch")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment(
                "test",
                vec![external_rule, global_rule],
                environment,
            )
            .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
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

        let mut global = linter.begin_project("/project").unwrap();
        let helper = global
            .add_source(SourceFile::new("helper.js", "export { fetch as send };"))
            .unwrap();
        let main = global
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        assert!(helper.is_empty());
        global
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
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
        [ResolutionResult::Internal {
            path: "helper.js".into(),
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
        [ResolutionResult::Internal {
            path: "sink.js".into(),
        }],
    );
        project.add_resolved(
        "main.js",
        "import { append } from './helper'; const element = document.createElement('script'); append(element);",
        [ResolutionResult::Internal {
            path: "helper.js".into(),
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
        [ResolutionResult::Internal {
            path: "helper.js".into(),
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
        [ResolutionResult::Internal {
            path: "helper.js".into(),
        }],
    );
        let report = project.finish();
        assert!(report.files.iter().all(|file| file.findings.is_empty()));
    }

    #[test]
    fn linked_unknown_exports_and_importer_reassignment_fail_closed() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        let helper = session
            .add_source(SourceFile::new(
                "helper.js",
                "import { request } from 'web'; export { request as send };",
            ))
            .unwrap();
        let main = session
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send = local; send();",
            ))
            .unwrap();
        session
            .record_resolution(
                helper[0].key.clone(),
                ResolutionResult::External {
                    package: "web".into(),
                },
            )
            .unwrap();
        session
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
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

        let mut missing = linter.begin_project("/project").unwrap();
        let main = missing
            .add_source(SourceFile::new(
                "main.js",
                "import { send } from './helper'; send();",
            ))
            .unwrap();
        missing
            .add_source(SourceFile::new("helper.js", "export const other = 1;"))
            .unwrap();
        missing
            .record_resolution(
                main[0].key.clone(),
                ResolutionResult::Internal {
                    path: "helper.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("./helper", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut session = linter.begin_project("/project").unwrap();
        session
            .add_source(SourceFile::new(
                "main.js",
                "import { request } from './helper'; request();",
            ))
            .unwrap();
        let report = session.finish().unwrap();

        assert!(report.files.iter().all(|file| file.findings.is_empty()));
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unresolved_internal_request".into())
        );
    }

    #[test]
    fn commonjs_export_aliases_preserve_external_provenance_across_modules() {
        let rule = Rule::builder("network.request")
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "const { request } = require('web'); exports.send = request;",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "const { send } = require('./helper'); send();",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "barrel.js",
            "export * from './helper';",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "import * as api from './barrel'; api.request();",
            [ResolutionResult::Internal {
                path: "barrel.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "async function run() { const api = await import('./helper'); api.request(); }",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::module_call("web", "request"))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "const { request } = require('web'); exports.send = () => request();",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "const { send } = require('./helper'); send();",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );
        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export function get() { return request; }",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "import { get } from './helper'; get()('/remote');",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
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
            .label("Uses request")
            .category("network")
            .severity(Severity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::call(
                CallMatcher::module_export("web", "request").static_string_arg(0),
            ))
            .build()
            .unwrap();
        let linter = crate::Linter::new(
            crate::RuleCatalog::with_environment("test", vec![rule], crate::Environment::default())
                .unwrap(),
        );

        let mut project = ProjectFixture::new(&linter);
        project.add_resolved(
            "helper.js",
            "import { request } from 'web'; export { request as send };",
            [ResolutionResult::External {
                package: "web".into(),
            }],
        );
        project.add_resolved(
            "main.js",
            "import { send } from './helper'; send('/remote');",
            [ResolutionResult::Internal {
                path: "helper.js".into(),
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
}

#[test]
fn project_keeps_sorted_parse_failures_separate_from_valid_modules() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    session
        .add_source(SourceFile::new("z.js", "function {"))
        .unwrap();
    session
        .add_source(SourceFile::new("a.js", "fetch('/remote');"))
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
    assert_eq!(report.files[1].parse_diagnostics.len(), 1);
}

#[test]
fn session_returns_static_import_dynamic_import_require_and_reexport_requests() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    let requests = session
            .add_source(SourceFile::new(
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
            ResolutionRequestKind::Import,
            ResolutionRequestKind::Import,
            ResolutionRequestKind::Require,
            ResolutionRequestKind::DynamicImport,
        ]
    );
    assert_eq!(requests[0].request, "./dep");
    assert_eq!(requests[2].request, "./cjs");
    assert_eq!(requests[3].request, "./lazy");
    assert_eq!(requests[2].key.range.start.column, 19);
    assert_eq!(requests[2].key.range.end.column, 26);
}

#[test]
fn session_rejects_resolution_for_an_unauthored_request() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    session
        .add_source(SourceFile::new("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.record_resolution(key("main.js"), ResolutionResult::Missing);
    assert!(matches!(error, Err(ProjectInputError::UnknownRequest(_))));
}

#[test]
fn rejected_duplicate_source_does_not_replace_the_original() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    session
        .add_source(SourceFile::new("main.js", "fetch('/remote');"))
        .unwrap();
    let error = session.add_source(SourceFile::new("./main.js", ""));
    assert!(matches!(error, Err(ProjectInputError::DuplicateSource(_))));

    let report = session.finish().unwrap();
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn type_only_reexports_do_not_create_runtime_requests() {
    let linter = test_linter();
    let mut session = linter.begin_project("/project").unwrap();
    let requests = session
        .add_source(SourceFile::new(
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
        [ResolutionResult::Internal {
            path: "dep.js".into(),
        }],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolutionResult::Internal {
            path: "barrel.js".into(),
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
        [ResolutionResult::Internal {
            path: "dep.js".into(),
        }],
    );
    missing.add("dep.js", "export const value = 1;");
    let report = missing.finish();
    assert_eq!(report.diagnostics[0].code, "missing_imported_export".into());
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
            ResolutionResult::Internal {
                path: "a.js".into(),
            },
            ResolutionResult::Internal {
                path: "b.js".into(),
            },
        ],
    );
    project.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolutionResult::Internal {
            path: "barrel.js".into(),
        }],
    );

    let report = project.finish();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ambiguous_star_export".into())
    );
}

#[test]
fn outside_project_targets_accept_normalized_absolute_paths() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import value from './outside';",
        [ResolutionResult::OutsideProject {
            path: "/other/./dependency.js".into(),
        }],
    );
    let report = project.finish();
    assert_eq!(report.diagnostics[0].code, "outside_project_target".into());
}

#[test]
fn dynamic_commonjs_export_shapes_are_reported_and_fail_closed() {
    let linter = test_linter();
    let mut project = ProjectFixture::new(&linter);
    project.add_resolved(
        "main.js",
        "import { value } from './dependency';",
        [ResolutionResult::Internal {
            path: "dependency.js".into(),
        }],
    );
    project.add("dependency.js", "module.exports = { value: 1, ...extra };");

    let report = project.finish();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unsupported_commonjs_exports".into())
    );
}
