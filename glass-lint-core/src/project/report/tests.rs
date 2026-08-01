use glass_lint_datastructures::{Position, SourceRange};

use super::*;
use crate::{
    RuleCatalog, RuleId, Severity,
    api::rule::{Category, Confidence, EventQuery, Rule, Severity as RuleSeverity},
    project::{
        AnalysisDiagnostic, AnalysisOperationCounts, Diagnostic, EvidenceRole, EvidenceStep,
        EvidenceTrace, EvidenceTraces, FileReport, Finding, MatchCertainty, ProjectRelativePath,
        ReportCompletion, SourceFile, SourceLocation,
    },
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
    Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "request detected".into(),
        Severity::Warning,
        SourceLocation::new(
            ProjectRelativePath::new("src/é.js").unwrap(),
            range(2, 4, 12),
        ),
        EvidenceTraces::new(vec![EvidenceTrace::new(vec![
            EvidenceStep::new(
                EvidenceRole::Occurrence,
                "source".into(),
                SourceLocation::new(
                    ProjectRelativePath::new("src/é.js").unwrap(),
                    range(1, 1, 3),
                ),
            ),
            EvidenceStep::new(
                EvidenceRole::Occurrence,
                "context".into(),
                SourceLocation::new(
                    ProjectRelativePath::new("src/é.js").unwrap(),
                    range(1, 1, 3),
                ),
            ),
        ])]),
        MatchCertainty::Definite,
    )
}

#[test]
fn qualifies_findings_and_preserves_missing_evidence_ranges() {
    let file = FileReport::new(
        ProjectRelativePath::new("src/é.js").unwrap(),
        vec![finding()],
        Vec::new(),
    );

    assert_eq!(file.path().as_str(), "src/é.js");
    assert_eq!(file.findings()[0].location().path().as_str(), "src/é.js");
    assert_eq!(
        file.findings()[0].evidence().traces()[0].steps()[0]
            .location()
            .path()
            .as_str(),
        "src/é.js"
    );
    assert_eq!(
        file.findings()[0].evidence().traces()[0].steps()[1]
            .location()
            .path()
            .as_str(),
        "src/é.js"
    );
}

fn report(path: &str, completion: ReportCompletion) -> AnalysisReport {
    AnalysisReport::new(
        crate::REPORT_VERSION,
        "test".into(),
        vec![FileReport::new(
            ProjectRelativePath::new(path).unwrap(),
            Vec::new(),
            Vec::new(),
        )],
        Vec::new(),
        AnalysisOperationCounts::default(),
        completion,
    )
}

#[test]
fn combine_reports_preserves_partial_without_parse_diagnostic() {
    let complete = report("a.js", ReportCompletion::Complete);
    let partial = AnalysisReport::new(
        crate::REPORT_VERSION,
        "test".into(),
        vec![FileReport::new(
            ProjectRelativePath::new("b.js").unwrap(),
            Vec::new(),
            vec![Diagnostic::project(AnalysisDiagnostic::new(
                crate::project::types::DiagnosticKind::FactsBudgetExhausted.into(),
                "facts exhausted".into(),
                None,
            ))],
        )],
        Vec::new(),
        AnalysisOperationCounts::default(),
        ReportCompletion::Partial,
    );

    let combined = AnalysisReport::combine([complete, partial]).unwrap();
    assert_eq!(combined.completion(), ReportCompletion::Partial);
    assert_eq!(
        combined.files()[1].diagnostics()[0].code(),
        "semantic_budget_exhausted"
    );
    assert!(
        combined
            .files()
            .iter()
            .all(|file| !file.has_parse_diagnostics())
    );
}

#[test]
fn combine_reports_preserves_report_and_file_diagnostics() {
    let parse_only = FileReport::new(
        ProjectRelativePath::new("broken.js").unwrap(),
        Vec::new(),
        vec![Diagnostic::parse(
            ProjectRelativePath::new("broken.js").unwrap(),
            crate::ParseDiagnostic {
                code: crate::project::types::DiagnosticKind::SyntaxError.into(),
                message: "invalid syntax".into(),
                filename: "broken.js".into(),
                range: None,
                failure: crate::parse::ParseFailureKind::Syntax,
            },
        )],
    );
    let partial = AnalysisReport::new(
        crate::REPORT_VERSION,
        "test".into(),
        vec![parse_only],
        vec![Diagnostic::project(AnalysisDiagnostic::new(
            crate::project::types::DiagnosticKind::LinkingBudgetExhausted.into(),
            "linking exhausted".into(),
            None,
        ))],
        AnalysisOperationCounts::default(),
        ReportCompletion::Partial,
    );
    let combined =
        AnalysisReport::combine([report("empty.js", ReportCompletion::Complete), partial]).unwrap();

    assert_eq!(combined.summary().files(), 2);
    assert_eq!(combined.summary().parse_diagnostics(), 1);
    assert_eq!(combined.files()[0].path().as_str(), "broken.js");
    assert_eq!(
        combined.diagnostics()[0].code(),
        "graph_link_budget_exhausted"
    );
}

#[test]
fn combine_reports_adds_all_operation_counts() {
    let first = AnalysisReport::new(
        crate::REPORT_VERSION,
        "test".into(),
        vec![FileReport::new(
            ProjectRelativePath::new("a.js").unwrap(),
            Vec::new(),
            Vec::new(),
        )],
        Vec::new(),
        AnalysisOperationCounts::new(1, 2, 3, 4, 5, 6, 7),
        ReportCompletion::Complete,
    );
    let second = AnalysisReport::new(
        crate::REPORT_VERSION,
        "test".into(),
        vec![FileReport::new(
            ProjectRelativePath::new("b.js").unwrap(),
            Vec::new(),
            Vec::new(),
        )],
        Vec::new(),
        AnalysisOperationCounts::new(usize::MAX, 20, 30, 40, 50, 60, 70),
        ReportCompletion::Complete,
    );
    let combined = AnalysisReport::combine([first, second]).unwrap();
    assert_eq!(
        combined.operations(),
        AnalysisOperationCounts::new(usize::MAX, 22, 33, 44, 55, 66, 77)
    );
}

#[test]
fn operation_counts_preserve_path_metrics_deterministically() {
    let mut first = AnalysisOperationCounts::new(0, 0, 0, 0, 0, 0, 0);
    first.set_path_metrics(8, 13, 5, 21, 3, 4);
    let mut second = AnalysisOperationCounts::new(0, 0, 0, 0, 0, 0, 0);
    second.set_path_metrics(4, 7, 2, 9, 6, 1);

    first += second;

    assert_eq!(first.max_live_alternatives(), 8);
    assert_eq!(first.trace_nodes(), 20);
    assert_eq!(first.trace_heads(), 7);
    assert_eq!(first.coalescing_comparisons(), 30);
    assert_eq!(first.fixed_point_iterations(), 9);
    assert_eq!(first.rendered_traces(), 5);
}

#[test]
fn combine_reports_rejects_schema_mismatch() {
    let first = report("a.js", ReportCompletion::Complete);
    let second = AnalysisReport::new(
        crate::REPORT_VERSION + 1,
        "test".into(),
        vec![FileReport::new(
            ProjectRelativePath::new("b.js").unwrap(),
            Vec::new(),
            Vec::new(),
        )],
        Vec::new(),
        AnalysisOperationCounts::default(),
        ReportCompletion::Complete,
    );
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
    let second = AnalysisReport::new(
        crate::REPORT_VERSION,
        "other".into(),
        vec![FileReport::new(
            ProjectRelativePath::new("b.js").unwrap(),
            Vec::new(),
            Vec::new(),
        )],
        Vec::new(),
        AnalysisOperationCounts::default(),
        ReportCompletion::Complete,
    );
    assert_eq!(
        AnalysisReport::combine([first, second]),
        Err(ReportCombineError::ToolVersionMismatch {
            expected: "test".into(),
            actual: "other".into(),
        })
    );
}

#[test]
fn shared_evidence_path_is_replaced_by_inline_steps() {
    let first = EvidenceStep::new(
        EvidenceRole::Occurrence,
        "related".into(),
        SourceLocation::new(ProjectRelativePath::new("dep.js").unwrap(), range(3, 1, 2)),
    );
    let traces = EvidenceTraces::new(vec![EvidenceTrace::new(vec![
        EvidenceStep::new(
            EvidenceRole::Occurrence,
            "source".into(),
            SourceLocation::new(
                ProjectRelativePath::new("src/é.js").unwrap(),
                range(1, 1, 3),
            ),
        ),
        first,
    ])]);
    let project_finding = Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "request detected".into(),
        Severity::Warning,
        SourceLocation::new(
            ProjectRelativePath::new("src/é.js").unwrap(),
            range(2, 4, 12),
        ),
        traces,
        MatchCertainty::Definite,
    );

    assert_eq!(project_finding.evidence().traces()[0].steps().len(), 2);
    assert_eq!(
        project_finding.evidence().traces()[0].steps()[1].message(),
        "related"
    );
}

#[test]
fn duplicate_findings_merge_traces_and_keep_definite_certainty() {
    let first = finding();
    let second = Finding::new(
        first.rule_id().clone(),
        first.message().to_owned(),
        first.severity(),
        first.location().clone(),
        EvidenceTraces::with_truncation(
            vec![EvidenceTrace::new(vec![EvidenceStep::new(
                EvidenceRole::Source,
                "source".into(),
                first.location().clone(),
            )])],
            true,
        ),
        MatchCertainty::Possible,
    );

    let merged = first.merge_duplicate(&second);
    assert_eq!(merged.certainty(), MatchCertainty::Definite);
    assert!(merged.evidence().truncated());
    assert_eq!(merged.evidence().traces().len(), 2);
    assert_eq!(
        merged.evidence().traces()[0].steps()[0].role(),
        EvidenceRole::Source
    );
}

#[test]
fn direct_qualification_matches_one_file_project_shape() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category(Category::new("network").unwrap())
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
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
    let (_, _, mut snippet_files, _, _, _) = linter
        .lint_source(source_file("main.js", source))
        .unwrap()
        .into_parts();
    let direct = snippet_files.remove(0);
    let mut manual_session = linter.begin_project().unwrap();
    manual_session
        .analyze_source(source_file("main.js", source))
        .unwrap();
    let manual = manual_session
        .finish_local()
        .resolve([])
        .unwrap()
        .finish()
        .unwrap();
    assert_eq!(direct, manual.files()[0].clone());
}

#[cfg(feature = "serde")]
#[test]
fn report_is_source_free_and_not_serialized() {
    let file = FileReport::new(
        ProjectRelativePath::new("main.js").unwrap(),
        Vec::new(),
        Vec::new(),
    );

    let json = serde_json::to_value(&file).unwrap();
    assert!(json.get("source").is_none());
}

#[cfg(feature = "serde")]
#[test]
fn match_certainty_serializes_as_stable_spellings() {
    let definite = serde_json::to_value(MatchCertainty::Definite).unwrap();
    let possible = serde_json::to_value(MatchCertainty::Possible).unwrap();
    assert_eq!(definite, "definite");
    assert_eq!(possible, "possible");
}

#[cfg(feature = "serde")]
#[test]
fn finding_serialization_includes_certainty() {
    let finding = Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "test".into(),
        Severity::Warning,
        SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        EvidenceTraces::new(vec![EvidenceTrace::new(vec![EvidenceStep::new(
            EvidenceRole::Occurrence,
            "test evidence".into(),
            SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        )])]),
        MatchCertainty::Definite,
    );
    let json = serde_json::to_value(&finding).unwrap();
    assert_eq!(json["certainty"], "definite");

    let possible = Finding::new(
        RuleId::parse("js:network.request").unwrap(),
        "test".into(),
        Severity::Warning,
        SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        EvidenceTraces::new(vec![EvidenceTrace::new(vec![EvidenceStep::new(
            EvidenceRole::Occurrence,
            "test evidence".into(),
            SourceLocation::new(ProjectRelativePath::new("main.js").unwrap(), range(1, 1, 2)),
        )])]),
        MatchCertainty::Possible,
    );
    let json = serde_json::to_value(&possible).unwrap();
    assert_eq!(json["certainty"], "possible");
}

#[cfg(feature = "serde")]
#[test]
fn snippet_serializes_as_one_analysis_file_without_source_text() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category(Category::new("network").unwrap())
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = crate::Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = crate::Linter::new(crate::LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();
    let report = linter
        .lint_source(source_file("main.js", "fetch('/');"))
        .unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
    assert!(json["files"][0].get("source").is_none());
    assert_eq!(
        json["files"][0]["findings"][0]["location"]["path"],
        "main.js"
    );
}

#[test]
fn parse_and_valid_sources_each_produce_one_file_report() {
    let rule = Rule::builder("network.request")
        .description("Uses fetch")
        .category(Category::new("network").unwrap())
        .severity(RuleSeverity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
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
    let mut collection = linter.begin_project().unwrap();
    collection
        .analyze_source(source_file("valid.js", "fetch('/a');"))
        .unwrap();
    collection
        .analyze_source(source_file("broken.js", "fetch("))
        .unwrap();
    let report = collection
        .finish_local()
        .resolve([])
        .unwrap()
        .finish()
        .unwrap();

    assert_eq!(report.files().len(), 2);
    let valid = report
        .files()
        .iter()
        .find(|f| f.path().as_str() == "valid.js")
        .unwrap();
    let broken = report
        .files()
        .iter()
        .find(|f| f.path().as_str() == "broken.js")
        .unwrap();

    assert_eq!(valid.findings().len(), 1);
    assert!(valid.diagnostics().is_empty());

    assert!(broken.findings().is_empty());
    assert!(broken.has_parse_diagnostics());
}
