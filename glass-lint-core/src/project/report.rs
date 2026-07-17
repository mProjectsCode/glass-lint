//! Project finding assembly and deterministic evidence ownership.

use super::{AnalysisReport, Evidence, Finding, ReportCompletion};

/// Why independently produced reports could not be combined losslessly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportCombineError {
    /// At least one report is required to define schema and tool identity.
    Empty,
    /// Every report in one aggregate must use the same schema contract.
    SchemaMismatch { expected: u32, actual: u32 },
    /// Reports from different tool versions are not silently mixed.
    ToolVersionMismatch { expected: String, actual: String },
}

impl std::fmt::Display for ReportCombineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("cannot combine an empty report collection"),
            Self::SchemaMismatch { expected, actual } => write!(
                formatter,
                "report schema mismatch: expected {expected}, found {actual}"
            ),
            Self::ToolVersionMismatch { expected, actual } => write!(
                formatter,
                "report tool version mismatch: expected {expected}, found {actual}"
            ),
        }
    }
}

impl std::error::Error for ReportCombineError {}

#[cfg(test)]
fn project_path(path: &str) -> super::types::ProjectRelativePath {
    super::types::ProjectRelativePath::new(path).unwrap()
}

impl AnalysisReport {
    /// Losslessly combine reports produced by independent analyses.
    ///
    /// ```
    /// # use glass_lint_core::{Linter, RuleCatalog, AnalysisReport};
    /// let linter = Linter::new(RuleCatalog::new("example", vec![]).unwrap());
    /// let first = linter.lint_snippet("", "first.js").unwrap();
    /// let second = linter.lint_snippet("", "second.js").unwrap();
    /// let combined = AnalysisReport::combine([first, second]).unwrap();
    /// assert_eq!(combined.files.len(), 2);
    /// ```
    pub fn combine(reports: impl IntoIterator<Item = Self>) -> Result<Self, ReportCombineError> {
        let mut reports = reports.into_iter();
        let Some(mut combined) = reports.next() else {
            return Err(ReportCombineError::Empty);
        };
        for mut report in reports {
            if report.schema_version != combined.schema_version {
                return Err(ReportCombineError::SchemaMismatch {
                    expected: combined.schema_version,
                    actual: report.schema_version,
                });
            }
            if report.tool_version != combined.tool_version {
                return Err(ReportCombineError::ToolVersionMismatch {
                    expected: combined.tool_version,
                    actual: report.tool_version,
                });
            }
            combined.files.append(&mut report.files);
            combined.diagnostics.append(&mut report.diagnostics);
            combined.operations += report.operations;
            if report.completion == ReportCompletion::Partial {
                combined.completion = ReportCompletion::Partial;
            }
        }
        combined
            .files
            .sort_by(|left, right| left.path.cmp(&right.path));
        combined.diagnostics.sort_by(|left, right| {
            (
                left.path().map(super::ProjectRelativePath::as_str),
                left.code(),
                left.message(),
            )
                .cmp(&(
                    right.path().map(super::ProjectRelativePath::as_str),
                    right.code(),
                    right.message(),
                ))
        });
        Ok(combined)
    }
}

impl Finding {
    /// Append related evidence and retain deterministic de-duplicated order.
    pub fn append_related(&mut self, evidence: impl IntoIterator<Item = Evidence>) {
        self.evidence.extend(evidence);
        let evidence = std::mem::take(&mut self.evidence);
        self.evidence = evidence.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Diagnostic, Evidence, FileReport, Finding, OperationCounts, Position, ProjectDiagnostic,
        ProjectRelativePath, RuleCatalog, RuleId, Severity, SourceFile, SourceLocation,
        SourceRange,
        api::rule::{Confidence, Matcher, Rule, Severity as RuleSeverity},
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
                path: project_path("src/é.js"),
                range: range(2, 4, 12),
            },
            evidence: vec![
                Evidence {
                    message: "source".into(),
                    count: 1,
                    evidence_truncated: false,
                    location: Some(SourceLocation {
                        path: project_path("src/é.js"),
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
            path: project_path("src/é.js"),
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
                path: project_path(path),
                findings: Vec::new(),
                diagnostics: Vec::new(),
            }],
            diagnostics: Vec::new(),
            operations: OperationCounts::default(),
            completion,
        }
    }

    #[test]
    fn combine_reports_preserves_partial_without_parse_diagnostic() {
        let complete = report("a.js", ReportCompletion::Complete);
        let mut partial = report("b.js", ReportCompletion::Partial);
        partial.files[0]
            .diagnostics
            .push(Diagnostic::project(ProjectDiagnostic {
                code: crate::project::types::DiagnosticKind::SemanticBudgetExhausted.into(),
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
            path: project_path("broken.js"),
            findings: Vec::new(),
            diagnostics: vec![Diagnostic::parse(
                project_path("broken.js"),
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
            .push(Diagnostic::project(ProjectDiagnostic {
                code: crate::project::types::DiagnosticKind::GraphLinkBudgetExhausted.into(),
                message: "linking exhausted".into(),
                location: None,
            }));
        let combined =
            AnalysisReport::combine([report("empty.js", ReportCompletion::Complete), partial])
                .unwrap();

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
        first.operations = OperationCounts {
            files: 1,
            requests: 2,
            edges: 3,
            exports: 4,
            scc_rounds: 5,
            effect_projections: 6,
            evidence: 7,
        };
        let mut second = report("b.js", ReportCompletion::Complete);
        second.operations = OperationCounts {
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
            OperationCounts {
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
    fn related_evidence_is_deduplicated_deterministically() {
        let mut project_finding = finding();
        let related = Evidence {
            message: "related".into(),
            count: 1,
            evidence_truncated: false,
            location: Some(SourceLocation {
                path: project_path("dep.js"),
                range: range(3, 1, 2),
            }),
        };
        project_finding.append_related([related.clone(), related]);

        assert_eq!(project_finding.evidence.len(), 3);
        assert_eq!(project_finding.evidence[2].message, "related");
    }

    #[test]
    fn direct_qualification_matches_one_file_project_shape() {
        let rule = Rule::builder("network.request")
            .label("Uses fetch")
            .category("network")
            .severity(RuleSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = crate::Linter::new(
            RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
        );
        let source = "fetch(\"https://example.test\");";
        let direct = linter
            .lint_snippet(source, "main.js")
            .unwrap()
            .files
            .into_iter()
            .next()
            .unwrap();
        let mut manual_session = linter.begin_analysis("/project").unwrap();
        manual_session
            .add_source(source_file("main.js", source))
            .unwrap();
        let manual = manual_session.finish().unwrap();
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
            .label("Uses fetch")
            .category("network")
            .severity(RuleSeverity::Warning)
            .confidence(Confidence::High)
            .matcher(Matcher::global_call("fetch"))
            .build()
            .unwrap();
        let mut environment = crate::Environment::default();
        environment.add_global("fetch").unwrap();
        let linter = crate::Linter::new(
            RuleCatalog::with_environment("test", vec![rule], environment).unwrap(),
        );
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
}
