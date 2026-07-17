use super::*;
use crate::AnalysisLimits;

fn configured_linter(limits: AnalysisLimits) -> crate::Linter {
    test_linter_with_selection(crate::RuleSelection::default(), limits)
}

fn configured_flow_linter(limits: AnalysisLimits) -> crate::Linter {
    crate::Linter::new(
        crate::LinterConfig::new(
            vec![flow_linter().catalog().clone()],
            flow_linter().analysis_environment().clone(),
        )
        .with_limits(limits),
    )
    .unwrap()
}

fn lint_one(linter: &crate::Linter, path: &str, source: &str) -> AnalysisReport {
    let mut session = linter.begin_analysis("/project").unwrap();
    session.add_source(source_file(path, source)).unwrap();
    session.finish().unwrap()
}

fn diagnostics(report: &AnalysisReport) -> Vec<(Option<&str>, &str)> {
    report
        .files
        .iter()
        .flat_map(|file| {
            file.diagnostics
                .iter()
                .map(|diagnostic| (Some(file.path.as_str()), diagnostic.code()))
        })
        .chain(report.diagnostics.iter().map(|diagnostic| {
            (
                diagnostic.path().map(ProjectRelativePath::as_str),
                diagnostic.code(),
            )
        }))
        .collect()
}

fn request_report(result: ResolverOutcome) -> AnalysisReport {
    let linter = test_linter();
    let mut fixture = ProjectFixture::new(&linter);
    fixture.add_resolved("main.js", "import value from './dep';", [result]);
    fixture.finish()
}

fn ambiguous_report() -> AnalysisReport {
    let linter = test_linter();
    let mut fixture = ProjectFixture::new(&linter);
    fixture.add("a.js", "export const value = 1;");
    fixture.add("b.js", "export const value = 2;");
    fixture.add_resolved(
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
    fixture.add_resolved(
        "main.js",
        "import { value } from './barrel';",
        [ResolverOutcome::Internal {
            path: project_path("barrel.js"),
        }],
    );
    fixture.finish()
}

#[test]
fn status_policy_matrix_has_expected_scope_and_completion() {
    let linter = test_linter();
    let syntax = lint_one(&linter, "syntax.js", "function {");
    let depth_limits = AnalysisLimits {
        syntax_depth: 1,
        ..AnalysisLimits::default()
    };
    let depth = lint_one(
        &configured_linter(depth_limits),
        "depth.js",
        "function f() { return (1); }",
    );
    let oversized = lint_one(
        &linter,
        "large.js",
        &"x".repeat(crate::MAX_SOURCE_BYTES + 1),
    );

    let mut dynamic = ProjectFixture::new(&linter);
    dynamic.add_resolved(
        "main.js",
        "import { value } from './dep';",
        [ResolverOutcome::Internal {
            path: project_path("dep.js"),
        }],
    );
    dynamic.add("dep.js", "module.exports = { value: 1, ...extra };");
    let dynamic = dynamic.finish();
    let missing = request_report(ResolverOutcome::Missing);
    let unsupported = request_report(ResolverOutcome::Unsupported {
        reason: "unsupported extension".into(),
    });
    let outside = request_report(ResolverOutcome::OutsideProject {
        path: "/other/dep.js".into(),
    });
    let ambiguous = ambiguous_report();

    let rows = [
        (&syntax, Some("syntax.js"), "syntax_error"),
        (&depth, Some("depth.js"), "syntax_depth_exceeded"),
        (&oversized, Some("large.js"), "source_too_large"),
        (&dynamic, Some("dep.js"), "unsupported_commonjs_exports"),
        (&missing, Some("main.js"), "unresolved_internal_request"),
        (&unsupported, Some("main.js"), "unsupported_project_target"),
        (&outside, Some("main.js"), "outside_project_target"),
        (&ambiguous, Some("main.js"), "ambiguous_star_export"),
    ];
    for (report, path, code) in rows {
        assert_eq!(report.completion, ReportCompletion::Partial, "{code}");
        assert_eq!(diagnostics(report), vec![(path, code)], "{code}");
        assert!(
            report.files.iter().all(|file| file.findings.is_empty()),
            "{code}"
        );
    }
}

#[test]
fn parse_status_and_structured_diagnostic_stay_consistent() {
    let report = lint_one(&test_linter(), "broken.js", "fetch(");
    assert_eq!(report.completion, ReportCompletion::Partial);
    let diagnostic = &report.files[0].diagnostics[0];
    assert!(diagnostic.parse_diagnostic().is_some());
    assert_eq!(diagnostic.path().unwrap().as_str(), "broken.js");
    let restored: AnalysisReport =
        serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
    assert_eq!(restored.completion, ReportCompletion::Partial);
    assert!(
        restored.files[0].diagnostics[0]
            .parse_diagnostic()
            .is_some()
    );
    assert_eq!(restored, report);
}

#[test]
fn external_and_builtin_requests_are_complete() {
    for result in [
        ResolverOutcome::External {
            package: "package".into(),
        },
        ResolverOutcome::Builtin {
            name: "node:fs".into(),
        },
    ] {
        let report = request_report(result);
        assert_eq!(report.completion, ReportCompletion::Complete);
        assert!(diagnostics(&report).is_empty());
    }
}

#[test]
fn proven_missing_export_is_diagnostic_but_complete() {
    let linter = test_linter();
    let mut fixture = ProjectFixture::new(&linter);
    fixture.add_resolved(
        "main.js",
        "import { missing } from './dep';",
        [ResolverOutcome::Internal {
            path: project_path("dep.js"),
        }],
    );
    fixture.add("dep.js", "export const present = 1;");
    let report = fixture.finish();
    assert_eq!(report.completion, ReportCompletion::Complete);
    assert_eq!(
        diagnostics(&report),
        vec![(Some("main.js"), "missing_imported_export")]
    );
}

fn assert_limit_triplet(
    configure: fn(AnalysisLimits) -> crate::Linter,
    component: fn(&mut AnalysisLimits) -> &mut usize,
    required: usize,
    code: &str,
    scope: Option<&str>,
    analyze: impl Fn(&crate::Linter) -> AnalysisReport,
) {
    assert!(required > 1);
    for (limit, expected) in [
        (required - 1, ReportCompletion::Partial),
        (required, ReportCompletion::Complete),
        (required + 1, ReportCompletion::Complete),
    ] {
        let mut limits = AnalysisLimits::default();
        *component(&mut limits) = limit;
        let report = analyze(&configure(limits));
        assert_eq!(report.completion, expected, "{code} limit={limit}");
        if expected == ReportCompletion::Partial {
            let status_diagnostics = diagnostics(&report)
                .into_iter()
                .filter(|(_, actual)| *actual == code)
                .collect::<Vec<_>>();
            assert_eq!(status_diagnostics, vec![(scope, code)]);
            assert!(report.files.iter().all(|file| file.findings.is_empty()));
        } else {
            assert!(
                diagnostics(&report)
                    .iter()
                    .all(|(_, actual)| *actual != code)
            );
        }
    }
}

#[test]
fn facts_effects_flow_and_link_limits_cover_below_at_above() {
    let facts = |linter: &crate::Linter| {
        lint_one(
            linter,
            "main.js",
            "function f(value) { return value; } f(1);",
        )
    };
    assert_limit_triplet(
        configured_linter,
        |limits| &mut limits.semantic_operations,
        7,
        "semantic_budget_exhausted",
        Some("main.js"),
        facts,
    );

    let effects =
        |linter: &crate::Linter| lint_one(linter, "main.js", "function f(value) { return value; }");
    assert_limit_triplet(
        configured_linter,
        |limits| &mut limits.effect_operations,
        3,
        "effect_size_budget_exhausted",
        Some("main.js"),
        effects,
    );

    let linking = |linter: &crate::Linter| {
        let mut fixture = ProjectFixture::new(linter);
        fixture.add_resolved(
            "main.js",
            "import { value } from './dep';",
            [ResolverOutcome::Internal {
                path: project_path("dep.js"),
            }],
        );
        fixture.add("dep.js", "export const value = 1; export const other = 2;");
        fixture.finish()
    };
    assert_limit_triplet(
        configured_linter,
        |limits| &mut limits.link_operations,
        2,
        "graph_link_budget_exhausted",
        None,
        linking,
    );

    let flow = |linter: &crate::Linter| {
        let mut fixture = ProjectFixture::new(linter);
        fixture.add(
            "helper.js",
            "export function append(element) { element.src = url; document.head.appendChild(element); }",
        );
        fixture.add_resolved(
            "main.js",
            "import { append } from './helper'; const element = document.createElement('script'); append(element);",
            [ResolverOutcome::Internal {
                path: project_path("helper.js"),
            }],
        );
        fixture.finish()
    };
    assert_limit_triplet(
        configured_flow_linter,
        |limits| &mut limits.flow_operations,
        2,
        "flow_link_budget_exhausted",
        None,
        flow,
    );
}

#[test]
fn partial_status_never_emits_unproved_strict_finding() {
    let rule = Rule::builder("network.request")
        .description("Uses request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::module_call("./dep", "request"))
        .build()
        .unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![crate::RuleCatalog::new("test", vec![rule]).unwrap()],
        crate::Environment::default(),
    ))
    .unwrap();
    let report = lint_one(
        &linter,
        "main.js",
        "import { request } from './dep'; request();",
    );
    assert_eq!(report.completion, ReportCompletion::Partial);
    assert!(report.files[0].findings.is_empty());
    assert_eq!(
        diagnostics(&report),
        vec![(Some("main.js"), "unresolved_internal_request")]
    );
}
