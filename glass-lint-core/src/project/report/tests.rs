use glass_lint_datastructures::{Position, SourceRange};

use super::*;
use crate::{
    RuleCatalog, RuleId, Severity,
    api::rule::{Confidence, EventQuery, Rule, Severity as RuleSeverity},
    project::{
        AnalysisDiagnostic, AnalysisOperationCounts, Diagnostic, EvidenceRole, EvidenceStep,
        EvidenceTrace, EvidenceTraces, FileReport, Finding, MatchCertainty, ProjectRelativePath,
        ReportCompletion, SourceFile, SourceLocation, SourceText, types::DiagnosticCode,
    },
};

fn source_file(path: impl Into<String>, source: impl Into<SourceText>) -> SourceFile {
    SourceFile::new(path, source).unwrap()
}

fn range(line: u32, start: u32, end: u32) -> SourceRange {
    SourceRange::new(
        Position::new(line, start).unwrap(),
        Position::new(line, end).unwrap(),
    )
    .unwrap()
}

fn operation_counts(
    files: usize,
    requests: usize,
    edges: usize,
    exports: usize,
    scc_rounds: usize,
    effect_projections: usize,
    evidence: usize,
) -> AnalysisOperationCounts {
    let mut counts = AnalysisOperationCounts::default();
    counts.record_files(files);
    counts.record_requests(requests);
    counts.record_edges(edges);
    counts.record_exports(exports);
    counts.record_scc_rounds(scc_rounds);
    counts.record_effect_projections(effect_projections);
    counts.record_evidence(evidence);
    counts
}

fn operation_counts_with_path(
    max_live_alternatives: usize,
    trace_nodes: usize,
    trace_heads: usize,
    coalescing_comparisons: usize,
    fixed_point_iterations: usize,
    rendered_traces: usize,
) -> AnalysisOperationCounts {
    let mut counts = AnalysisOperationCounts::default();
    counts.record_max_live_alternatives(max_live_alternatives);
    counts.record_trace_nodes(trace_nodes);
    counts.record_trace_heads(trace_heads);
    counts.record_coalescing_comparisons(coalescing_comparisons);
    counts.record_fixed_point_iterations(fixed_point_iterations);
    counts.record_rendered_traces(rendered_traces);
    counts
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
        EvidenceTraces::new(vec![
            EvidenceTrace::new(vec![
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
            ])
            .unwrap(),
        ])
        .unwrap(),
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
                crate::project::types::DiagnosticKind::FactCapacityExhausted.into(),
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
        "semantic_fact_capacity_exhausted"
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
            crate::ParseDiagnostic::new(
                crate::parse::ParseFailureKind::Syntax,
                "invalid syntax",
                "stale-parser-name.js",
                None,
            ),
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
        combined.files()[0].diagnostics()[0]
            .path()
            .unwrap()
            .as_str(),
        "broken.js"
    );
    assert_eq!(
        combined.files()[0].diagnostics()[0]
            .parse_diagnostic()
            .unwrap()
            .filename(),
        "stale-parser-name.js"
    );
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
        operation_counts(1, 2, 3, 4, 5, 6, 7),
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
        operation_counts(usize::MAX, 20, 30, 40, 50, 60, 70),
        ReportCompletion::Complete,
    );
    let combined = AnalysisReport::combine([first, second]).unwrap();
    assert_eq!(
        combined.operations(),
        operation_counts(usize::MAX, 22, 33, 44, 55, 66, 77)
    );
}

#[test]
fn operation_counts_preserve_path_metrics_deterministically() {
    let mut first = operation_counts_with_path(8, 13, 5, 21, 3, 4);
    let second = operation_counts_with_path(4, 7, 2, 9, 6, 1);

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
fn combine_reports_rejects_duplicate_file_paths_transactionally() {
    let first = report("same.js", ReportCompletion::Complete);
    let second = report("same.js", ReportCompletion::Complete);

    assert_eq!(
        AnalysisReport::combine([first, second]),
        Err(ReportCombineError::DuplicateFilePath {
            path: ProjectRelativePath::new("same.js").unwrap(),
        })
    );
}

#[test]
fn public_report_transformations_preserve_diagnostic_order() {
    let later_code = DiagnosticCode::new("z_project_diagnostic").unwrap();
    let earlier_code = DiagnosticCode::new("a_project_diagnostic").unwrap();
    let report = report("a.js", ReportCompletion::Complete)
        .with_project_diagnostics(&later_code, ["later code".into()])
        .with_project_diagnostics(&earlier_code, ["earlier code".into()])
        .into_partial("partial");

    assert_eq!(
        report
            .diagnostics()
            .iter()
            .map(Diagnostic::code)
            .collect::<Vec<_>>(),
        [
            "a_project_diagnostic",
            "incomplete_project",
            "z_project_diagnostic"
        ]
    );
}

#[test]
fn shared_evidence_path_is_replaced_by_inline_steps() {
    let first = EvidenceStep::new(
        EvidenceRole::Occurrence,
        "related".into(),
        SourceLocation::new(ProjectRelativePath::new("dep.js").unwrap(), range(3, 1, 2)),
    );
    let traces = EvidenceTraces::new(vec![
        EvidenceTrace::new(vec![
            EvidenceStep::new(
                EvidenceRole::Occurrence,
                "source".into(),
                SourceLocation::new(
                    ProjectRelativePath::new("src/é.js").unwrap(),
                    range(1, 1, 3),
                ),
            ),
            first,
        ])
        .unwrap(),
    ])
    .unwrap();
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
        EvidenceTraces::from_truncated(vec![
            EvidenceTrace::new(vec![EvidenceStep::new(
                EvidenceRole::Source,
                "source".into(),
                first.location().clone(),
            )])
            .unwrap(),
        ]),
        MatchCertainty::Possible,
    );

    let merged = first.merge_duplicate(second);
    assert_eq!(merged.certainty(), MatchCertainty::Definite);
    assert!(merged.evidence().truncated());
    assert_eq!(merged.evidence().traces().len(), 2);
    assert_eq!(
        merged.evidence().traces()[0].steps()[0].role(),
        EvidenceRole::Source
    );
}

#[path = "tests_extended.rs"]
mod tests_extended;
