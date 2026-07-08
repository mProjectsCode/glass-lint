use std::{collections::BTreeMap, path::Path};

use anyhow::Result;
use glass_lint_core::Finding;

use crate::{
    adapters::Adapter,
    cases::load_cases,
    types::{CaseResult, DiagnosticExpectation, SuiteReport, ToolExpectation, ToolResult},
};

pub fn run_suite(root: &Path, adapters: &[Box<dyn Adapter>]) -> Result<SuiteReport> {
    let cases = load_cases(root)?;
    let mut results = Vec::new();
    for case in cases {
        let mut tools = BTreeMap::new();
        for adapter in adapters {
            let version = adapter
                .version()
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let Some(expectation) = case.tools.get(adapter.name()) else {
                tools.insert(
                    adapter.name().into(),
                    ToolResult {
                        version,
                        skipped: true,
                        skip_reason: Some("tool not configured for this case".into()),
                        passed: true,
                        findings: vec![],
                        errors: vec![],
                    },
                );
                continue;
            };
            let (findings, errors) = match adapter.run(&case, expectation) {
                Ok(findings) => {
                    let errors = compare(&findings, expectation);
                    (findings, errors)
                }
                Err(error) => (vec![], vec![error.to_string()]),
            };
            tools.insert(
                adapter.name().into(),
                ToolResult {
                    version,
                    skipped: false,
                    skip_reason: None,
                    passed: errors.is_empty(),
                    findings,
                    errors,
                },
            );
        }
        results.push(CaseResult {
            id: case.id,
            description: case.description,
            source: case.source,
            tools,
        });
    }
    Ok(SuiteReport {
        schema_version: 1,
        cases: results,
    })
}

fn matches(finding: &Finding, expected: &DiagnosticExpectation) -> bool {
    finding.rule_id.as_str() == expected.rule_id
        && expected
            .message_id
            .as_ref()
            .is_none_or(|id| &finding.message_id == id)
        && expected
            .severity
            .is_none_or(|severity| finding.severity == severity)
        && expected
            .line
            .is_none_or(|line| finding.range.start.line == line)
        && expected
            .column
            .is_none_or(|column| finding.range.start.column == column)
        && expected
            .message
            .as_ref()
            .is_none_or(|message| &finding.message == message)
}

fn compare(findings: &[Finding], expectation: &ToolExpectation) -> Vec<String> {
    let mut errors = Vec::new();
    for expected in &expectation.required {
        let actual = findings
            .iter()
            .filter(|finding| matches(finding, expected))
            .count();
        if expected.count.is_some_and(|count| actual != count) {
            errors.push(format!(
                "expected {} x {}, found {}",
                expected.count.unwrap(),
                expected.rule_id,
                actual
            ));
        }
    }
    for forbidden in &expectation.forbidden {
        let actual = findings
            .iter()
            .filter(|finding| matches(finding, forbidden))
            .count();
        if actual > 0 {
            errors.push(format!(
                "forbidden diagnostic {} appeared {} time(s)",
                forbidden.rule_id, actual
            ));
        }
    }
    for finding in findings {
        if !expectation
            .required
            .iter()
            .any(|expected| matches(finding, expected))
        {
            errors.push(format!(
                "unexpected {}:{} at {:?}",
                finding.rule_id, finding.message_id, finding.range
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use glass_lint_core::Severity;

    use super::*;

    fn finding() -> Finding {
        Finding {
            rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
            message_id: "m".into(),
            message: "text".into(),
            severity: Severity::Warning,
            range: glass_lint_core::SourceRange {
                start: glass_lint_core::Position { line: 2, column: 3 },
                end: glass_lint_core::Position { line: 2, column: 4 },
            },
            evidence: vec![],
        }
    }

    #[test]
    fn finds_missing_diagnostic() {
        let expected = ToolExpectation {
            rules: vec![],
            required: vec![DiagnosticExpectation {
                rule_id: "test:a.b".into(),
                message_id: None,
                severity: None,
                count: Some(2),
                line: None,
                column: None,
                message: None,
            }],
            forbidden: vec![],
        };
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }

    #[test]
    fn flags_unexpected_diagnostic() {
        let expected = ToolExpectation {
            rules: vec![],
            required: vec![],
            forbidden: vec![],
        };
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }
}
