use super::*;
use crate::{
    AnalysisDiagnostic, AnalysisOperationCounts, Diagnostic, Evidence, FileReport, Finding,
    Position, ProjectRelativePath, RuleCatalog, RuleId, Severity, SourceFile, SourceLocation,
    SourceRange,
    api::rule::{Confidence, MatcherDecl, Rule, Severity as RuleSeverity},
};

fn source_file(path: impl Into<String>, source: impl Into<String>) -> SourceFile {
    SourceFile::new(path, source).unwrap()
}

fn range(line: u32, start: u32, end: u32) -> SourceRange {
    SourceRange::new(
        Position::new(line, start).unwrap(),
        Position::new(line, end).unwrap(),
    )
    .unwrap()
}

fn finding() -> Finding {
    Finding {
        rule_id: RuleId::parse("js:network.request").unwrap(),
        message_id: "detected".into(),
        message: "request detected".into(),
        severity: Severity::Warning,
        location: SourceLocation {
            path: ProjectRelativePath::new("src/é.js").unwrap(),
            range: range(2, 4, 12),
        },
        evidence: vec![
            Evidence {
                message: "source".into(),
                count: 1,
                evidence_truncated: false,
                location: Some(SourceLocation {
                    path: ProjectRelativePath::new("src/é.js").unwrap(),
                    range: range(1, 1, 3),
                }),
            },
            Evidence {
                message: "context".into(),
                count: 1,
                evidence_truncated: false,
                location: None,
            },
        ]
        .into_iter()
        .collect(),
    }
}

#[test]
fn qualifies_findings_and_preserves_missing_evidence_ranges() {
    let file = FileReport {
        path: ProjectRelativePath::new("src/é.js").unwrap(),
        findings: vec![finding()],
        diagnostics: Vec::new(),
    };

    assert_eq!(file.path, "src/é.js");
    assert_eq!(file.findings[0].location.path, "src/é.js");
    assert_eq!(
        file.findings[0].evidence[0].location.as_ref().unwrap().path,
        "src/é.js"
    );
    assert!(file.findings[0].evidence[1].location.is_none());
}

fn report(path: &str, completion: ReportCompletion) -> AnalysisReport {
    AnalysisReport {
        schema_version: crate::REPORT_VERSION,
        tool_version: "test".into(),
        files: vec![FileReport {
            path: ProjectRelativePath::new(path).unwrap(),
            findings: Vec::new(),
            diagnostics: Vec::new(),
        }],
        diagnostics: Vec::new(),
        operations: AnalysisOperationCounts::default(),
        completion,
    }
}

#[test]
fn combine_reports_preserves_partial_without_parse_diagnostic() {
    let complete = report("a.js", ReportCompletion::Complete);
    let mut partial = report("b.js", ReportCompletion::Partial);
    partial.files[0]
        .diagnostics
        .push(Diagnostic::project(AnalysisDiagnostic {
            code: crate::project::types::DiagnosticKind::FactsBudgetExhausted.into(),
            message: "facts exhausted".into(),
            location: None,
        }));

    let combined = AnalysisReport::combine([complete, partial]).unwrap();
    assert_eq!(combined.completion, ReportCompletion::Partial);
    assert_eq!(
        combined.files[1].diagnostics[0].code(),
        "semantic_budget_exhausted"
    );
    assert!(
        combined
            .files
            .iter()
            .all(|file| !file.has_parse_diagnostics())
    );
}

#[test]
fn combine_reports_preserves_report_and_file_diagnostics() {
    let parse_only = FileReport {
        path: ProjectRelativePath::new("broken.js").unwrap(),
        findings: Vec::new(),
        diagnostics: vec![Diagnostic::parse(
            ProjectRelativePath::new("broken.js").unwrap(),
            crate::ParseDiagnostic {
                code: crate::project::types::DiagnosticKind::SyntaxError.into(),
                message: "invalid syntax".into(),
                filename: "broken.js".into(),
                range: None,
            },
        )],
    };
    let mut partial = report("placeholder.js", ReportCompletion::Partial);
    partial.files = vec![parse_only];
    partial
        .diagnostics
        .push(Diagnostic::project(AnalysisDiagnostic {
            code: crate::project::types::DiagnosticKind::LinkingBudgetExhausted.into(),
            message: "linking exhausted".into(),
            location: None,
        }));
    let combined =
        AnalysisReport::combine([report("empty.js", ReportCompletion::Complete), partial]).unwrap();

    assert_eq!(combined.summary().files, 2);
    assert_eq!(combined.summary().parse_diagnostics, 1);
    assert_eq!(combined.files[0].path, "broken.js");
    assert_eq!(
        combined.diagnostics[0].code(),
        "graph_link_budget_exhausted"
    );
}

#[test]
fn combine_reports_adds_all_operation_counts() {
    let mut first = report("a.js", ReportCompletion::Complete);
    first.operations = AnalysisOperationCounts {
        files: 1,
        requests: 2,
        edges: 3,
        exports: 4,
        scc_rounds: 5,
        effect_projections: 6,
        evidence: 7,
    };
    let mut second = report("b.js", ReportCompletion::Complete);
    second.operations = AnalysisOperationCounts {
        files: usize::MAX,
        requests: 20,
        edges: 30,
        exports: 40,
        scc_rounds: 50,
        effect_projections: 60,
        evidence: 70,
    };
    let combined = AnalysisReport::combine([first, second]).unwrap();
    assert_eq!(
        combined.operations,
        AnalysisOperationCounts {
            files: usize::MAX,
            requests: 22,
            edges: 33,
            exports: 44,
            scc_rounds: 55,
            effect_projections: 66,
            evidence: 77,
        }
    );
}

#[test]
fn combine_reports_rejects_schema_mismatch() {
    let first = report("a.js", ReportCompletion::Complete);
    let mut second = report("b.js", ReportCompletion::Complete);
    second.schema_version += 1;
    assert_eq!(
        AnalysisReport::combine([first, second]),
        Err(ReportCombineError::SchemaMismatch {
            expected: crate::REPORT_VERSION,
            actual: crate::REPORT_VERSION + 1,
        })
    );
}

#[test]
fn combine_reports_rejects_tool_version_mismatch() {
    let first = report("a.js", ReportCompletion::Complete);
    let mut second = report("b.js", ReportCompletion::Complete);
    second.tool_version = "other".into();
    assert_eq!(
        AnalysisReport::combine([first, second]),
        Err(ReportCombineError::ToolVersionMismatch {
            expected: "test".into(),
            actual: "other".into(),
        })
    );
}

#[test]
fn shared_evidence_is_set_without_cloning_into_local() {
    let mut project_finding = finding();
    let shared: std::sync::Arc<[Evidence]> = vec![Evidence {
        message: "related".into(),
        count: 1,
        evidence_truncated: false,
        location: Some(SourceLocation {
            path: ProjectRelativePath::new("dep.js").unwrap(),
            range: range(3, 1, 2),
        }),
    }]
    .into();
    project_finding.set_shared_evidence(std::sync::Arc::clone(&shared));

    assert_eq!(project_finding.evidence.len(), 3);
    assert_eq!(project_finding.evidence[2].message, "related");
}

#[test]
fn direct_qualification_matches_one_file_project_shape() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category("network")
        .severity(RuleSeverity::Warning)
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
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();
    let source = "fetch(\"https://example.test\");";
    let direct = linter
        .lint_snippet(source, "main.js")
        .unwrap()
        .files
        .into_iter()
        .next()
        .unwrap();
    let mut manual_session = linter.begin_project("/project").unwrap();
    manual_session
        .analyze_source(source_file("main.js", source))
        .unwrap();
    let manual = manual_session
        .finish_local()
        .resolve([])
        .unwrap()
        .finish()
        .unwrap();
    let project = linter
        .lint_project(crate::ProjectInput {
            root: "/project".into(),
            sources: vec![source_file("main.js", source)],
            resolutions: Vec::new(),
        })
        .unwrap();

    assert_eq!(direct, project.files[0]);
    assert_eq!(direct, manual.files[0]);
}

#[test]
fn report_is_source_free_and_not_serialized() {
    let file = FileReport {
        path: ProjectRelativePath::new("main.js").unwrap(),
        findings: Vec::new(),
        diagnostics: Vec::new(),
    };

    let json = serde_json::to_value(&file).unwrap();
    assert!(json.get("source").is_none());
}

#[test]
fn snippet_serializes_as_one_analysis_file_without_source_text() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category("network")
        .severity(RuleSeverity::Warning)
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
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();
    let report = linter.lint_snippet("fetch('/');", "main.js").unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
    assert!(json["files"][0].get("source").is_none());
    assert_eq!(
        json["files"][0]["findings"][0]["location"]["path"],
        "main.js"
    );
    let serialized = serde_json::to_string(&report).unwrap();
    let round_trip: AnalysisReport = serde_json::from_str(&serialized).unwrap();
    assert_eq!(report, round_trip);
}

#[test]
fn parse_and_valid_sources_each_produce_one_file_report() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category("network")
        .severity(RuleSeverity::Warning)
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
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();

    // One valid file, one parse-failure file
    let report = linter
        .lint_project(crate::ProjectInput {
            root: "/project".into(),
            sources: vec![
                source_file("valid.js", "fetch('/a');"),
                source_file("broken.js", "fetch("),
            ],
            resolutions: Vec::new(),
        })
        .unwrap();

    assert_eq!(report.files.len(), 2);
    let valid = report.files.iter().find(|f| f.path == "valid.js").unwrap();
    let broken = report.files.iter().find(|f| f.path == "broken.js").unwrap();

    assert_eq!(valid.findings.len(), 1);
    assert!(valid.diagnostics.is_empty());

    assert!(broken.findings.is_empty());
    assert!(broken.has_parse_diagnostics());
}
