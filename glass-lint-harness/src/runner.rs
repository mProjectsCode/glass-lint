//! Case execution and expectation comparison.
//!
//! The runner records one result per case/tool, treating skipped tools as
//! explicit successful non-runs and preserving adapter timing by name.

use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::Result;
use glass_lint_core::Finding;
use tracing::info;

use crate::{
    adapters::Adapter,
    cases::load_cases,
    types::{
        CaseResult, ExpectedCount, FindingExpectation, SuiteReport, ToolExpectation, ToolResult,
    },
};

pub type AdapterTimings = BTreeMap<String, Duration>;

/// Execute every configured adapter against every discovered case.
pub fn run_suite(
    root: &Path,
    adapters: &[Box<dyn Adapter>],
) -> Result<(SuiteReport, Vec<AdapterTimings>)> {
    let cases = load_cases(root)?;
    let mut results = Vec::new();
    let mut all_timings = Vec::new();
    for case in &cases {
        let mut tools = BTreeMap::new();
        let mut timings = BTreeMap::new();
        for adapter in adapters {
            let tool_start = Instant::now();
            let version = adapter
                .version()
                .unwrap_or_else(|error| format!("unknown ({error})"));
            let Some(expectation) = case.adapters.get(adapter.name()) else {
                timings.insert(adapter.name().into(), tool_start.elapsed());
                tools.insert(
                    adapter.name().into(),
                    ToolResult::skipped(version, Some("tool not configured for this case".into())),
                );
                continue;
            };
            if case.project.is_some() && !adapter.supports_projects() {
                let reason = "adapter does not support project-shaped requests".to_string();
                timings.insert(adapter.name().into(), tool_start.elapsed());
                tools.insert(
                    adapter.name().into(),
                    ToolResult::skipped(version, Some(reason)),
                );
                continue;
            }
            let (findings, errors, operational_errors) =
                match adapter.run_with_locations(case, expectation) {
                    Ok(output) => {
                        let errors = compare(&output.findings, expectation);
                        (output.findings, errors, vec![])
                    }
                    Err(error) => (vec![], vec![], vec![error.to_string()]),
                };
            timings.insert(adapter.name().into(), tool_start.elapsed());
            tools.insert(
                adapter.name().into(),
                ToolResult {
                    version,
                    skipped: false,
                    skip_reason: None,
                    passed: errors.is_empty() && operational_errors.is_empty(),
                    findings,
                    mismatches: errors,
                    operational_errors,
                },
            );
        }
        let total: Duration = timings.values().sum();
        let details = timings
            .iter()
            .map(|(name, dur)| format!("{name} {dur:.1?}"))
            .collect::<Vec<_>>()
            .join(", ");
        info!(progress = format!("  {}: {total:.1?} ({})", case.id, details));
        all_timings.push(timings);
        results.push(CaseResult {
            id: case.id.clone(),
            description: case.description.clone(),
            source: case.source.clone(),
            adapters: tools,
        });
    }
    Ok((
        SuiteReport {
            schema_version: 1,
            cases: results,
        },
        all_timings,
    ))
}

impl FindingExpectation {
    fn matches(&self, finding: &Finding) -> bool {
        finding.rule_id == self.rule_id
            && self
                .message_id
                .as_ref()
                .is_none_or(|id| &finding.message_id == id)
            && self
                .severity
                .is_none_or(|severity| finding.severity == severity)
            && self
                .line
                .is_none_or(|line| finding.location.range.start().line() == line)
            && self
                .column
                .is_none_or(|column| finding.location.range.start().column() == column)
            && self
                .message
                .as_ref()
                .is_none_or(|message| &finding.message == message)
            && self
                .path
                .as_ref()
                .is_none_or(|path| finding.location.path == *path)
    }
}

fn compare(findings: &[Finding], expectation: &ToolExpectation) -> Vec<String> {
    let mut errors = Vec::new();
    for expected in expectation.required() {
        let actual = findings
            .iter()
            .enumerate()
            .filter(|(_, finding)| expected.matches(finding))
            .count();
        let count_matches = match expected.count {
            ExpectedCount::Exactly(count) => actual == count,
            ExpectedCount::AtLeastOne => actual > 0,
        };
        if !count_matches {
            errors.push(format!(
                "expected {:?} x {}, found {}",
                expected.count, expected.rule_id, actual
            ));
        }
    }
    for forbidden in expectation.forbidden() {
        let actual = findings
            .iter()
            .enumerate()
            .filter(|(_, finding)| forbidden.matches(finding))
            .count();
        if actual > 0 {
            errors.push(format!(
                "forbidden diagnostic {} appeared {} time(s)",
                forbidden.rule_id, actual
            ));
        }
    }
    for finding in findings {
        let is_required = expectation
            .required()
            .iter()
            .any(|expected| expected.matches(finding));
        let is_forbidden = expectation
            .forbidden()
            .iter()
            .any(|forbidden| forbidden.matches(finding));
        if !is_required && !is_forbidden {
            errors.push(format!(
                "unexpected {}:{} at {:?}",
                finding.rule_id, finding.message_id, finding.location.range
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use glass_lint_core::Severity;

    use super::*;
    use crate::types::ToolSelector;

    fn finding() -> Finding {
        Finding {
            rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
            message_id: "m".into(),
            message: "text".into(),
            severity: Severity::Warning,
            location: glass_lint_core::SourceLocation {
                path: glass_lint_core::ProjectRelativePath::new("main.js").unwrap(),
                range: glass_lint_core::SourceRange::new(
                    glass_lint_core::Position::new(2, 3).unwrap(),
                    glass_lint_core::Position::new(2, 4).unwrap(),
                )
                .unwrap(),
            },
            evidence: Vec::new().into_iter().collect(),
        }
    }

    #[test]
    fn finds_missing_diagnostic() {
        let expected = ToolExpectation::from_selector(
            ToolSelector::Rules(vec!["test:a.b".into()]),
            vec![FindingExpectation {
                path: None,
                rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
                message_id: None,
                severity: None,
                count: ExpectedCount::Exactly(2),
                line: None,
                column: None,
                message: None,
            }],
            vec![],
        )
        .unwrap();
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }

    #[test]
    fn flags_unexpected_diagnostic() {
        let expected = ToolExpectation::from_selector(
            ToolSelector::Config("heuristic".into()),
            vec![],
            vec![],
        )
        .unwrap();
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }

    #[test]
    fn reports_forbidden_diagnostic_once() {
        let expected = ToolExpectation::from_selector(
            ToolSelector::Rules(vec!["test:a.b".into()]),
            vec![],
            vec![FindingExpectation {
                path: None,
                rule_id: glass_lint_core::RuleId::parse("test:a.b").unwrap(),
                message_id: None,
                severity: None,
                count: ExpectedCount::Exactly(1),
                line: None,
                column: None,
                message: None,
            }],
        )
        .unwrap();
        assert_eq!(compare(&[finding()], &expected).len(), 1);
    }
}
