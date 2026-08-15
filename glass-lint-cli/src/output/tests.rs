use glass_lint_core::{
    AnalysisLimits, Environment, Linter, Rule, RuleCatalog, Severity,
    project::{AnalysisReport, ReportCompletion},
    rules::{Confidence, EventQuery},
};

use super::*;

fn linter(semantic_operations: usize) -> Linter {
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    Linter::new(
        glass_lint_core::LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            environment,
        )
        .with_limits(
            AnalysisLimits::default()
                .with_semantic_operations(semantic_operations)
                .unwrap(),
        ),
    )
    .unwrap()
}

fn output(path: &str, source: &str, report: AnalysisReport) -> FileOutput {
    FileOutput {
        path: path.into(),
        report,
        source: source.into(),
    }
}

fn json(files: &[FileOutput]) -> AnalysisReport {
    AnalysisReport::combine(files.iter().map(|file| file.report.clone())).unwrap()
}

#[test]
fn table_aligns_columns_without_padding_the_last_column() {
    let mut table = Table::new(["ID", "SEVERITY", "DESCRIPTION"]);
    table.push(Row::new(["x", "warning", "short"])).unwrap();

    let mut output = Vec::new();
    table.write(&mut output).unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "ID  SEVERITY  DESCRIPTION\nx   warning   short\n"
    );
}

#[test]
fn snippet_json_completion_matches_cli_exit_decision() {
    let source = "fetch('/remote');";
    let report = linter(1)
        .lint_source(SourceFile::new("partial.js", source).unwrap())
        .unwrap();
    let cli_failed = report.completion() == ReportCompletion::Partial
        || !report.diagnostics().is_empty()
        || report
            .files()
            .iter()
            .any(|file| !file.diagnostics().is_empty());
    let combined = json(&[output("partial.js", source, report)]);

    assert!(cli_failed);
    assert_eq!(combined.completion(), ReportCompletion::Partial);
    assert_eq!(
        combined.files()[0].diagnostics()[0].code(),
        "semantic_step_budget_exhausted"
    );
}

#[test]
fn mixed_complete_parse_partial_and_semantic_partial_json_is_stable() {
    let complete_source = "fetch('/ok');";
    let broken_source = "fetch(";
    let semantic_source = "fetch('/partial');";
    let complete = linter(64)
        .lint_source(SourceFile::new("a.js", complete_source).unwrap())
        .unwrap();
    let parse_partial = linter(64)
        .lint_source(SourceFile::new("b.js", broken_source).unwrap())
        .unwrap();
    let semantic_partial = linter(1)
        .lint_source(SourceFile::new("c.js", semantic_source).unwrap())
        .unwrap()
        .into_partial("project scope retained");
    let files = [
        output("c.js", semantic_source, semantic_partial),
        output("a.js", complete_source, complete),
        output("b.js", broken_source, parse_partial),
    ];

    let first = json(&files);
    let second = json(&files);
    assert_eq!(first, second);
    assert_eq!(first.completion(), ReportCompletion::Partial);
    assert_eq!(
        first
            .files()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        vec!["a.js", "b.js", "c.js"]
    );
    assert_eq!(first.summary().parse_diagnostics(), 1);
    assert_eq!(first.summary().file_diagnostics(), 1);
    assert_eq!(first.diagnostics()[0].code(), "incomplete_project");
}
