//! Project finding assembly and deterministic evidence ownership.

use super::{ProjectEvidence, ProjectFileReport, ProjectFinding, ProjectReport, SourceLocation};

impl ProjectFileReport {
    /// Qualify a single-file report with its owning project path.
    pub fn from_lint_report(path: impl Into<String>, report: crate::LintReport) -> Self {
        let path = path.into();
        Self {
            path: path.clone(),
            findings: report
                .findings
                .into_iter()
                .map(|finding| ProjectFinding::from_finding(finding, &path))
                .collect(),
            parse_diagnostics: report.parse_diagnostics,
        }
    }
}

impl ProjectReport {
    /// Assemble a project-shaped report when all files have already been
    /// linted independently and no project diagnostics exist.
    pub fn from_file_reports(
        tool_version: impl Into<String>,
        files: Vec<ProjectFileReport>,
    ) -> Self {
        let evidence = files
            .iter()
            .map(|file| {
                file.findings
                    .iter()
                    .map(|finding| finding.evidence.len())
                    .sum::<usize>()
            })
            .sum();
        Self {
            schema_version: crate::REPORT_VERSION,
            tool_version: tool_version.into(),
            operations: super::ProjectOperationCounts {
                files: files.len(),
                evidence,
                ..super::ProjectOperationCounts::default()
            },
            files,
            diagnostics: Vec::new(),
        }
    }
}

impl ProjectFinding {
    pub(crate) fn from_finding(finding: crate::Finding, path: &str) -> Self {
        Self {
            rule_id: finding.rule_id,
            message_id: finding.message_id,
            message: finding.message,
            severity: finding.severity,
            location: SourceLocation {
                path: path.to_owned(),
                range: finding.range,
            },
            evidence: finding
                .evidence
                .into_iter()
                .map(|evidence| ProjectEvidence {
                    message: evidence.message,
                    location: evidence.range.map(|range| SourceLocation {
                        path: path.to_owned(),
                        range,
                    }),
                    source: evidence.source,
                })
                .collect(),
        }
    }

    pub(crate) fn append_related(&mut self, evidence: impl IntoIterator<Item = ProjectEvidence>) {
        self.evidence.extend(evidence);
        let evidence = std::mem::take(&mut self.evidence);
        self.evidence = evidence.into_iter().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Evidence, Finding, LintReport, Position, RuleCatalog, RuleId, Severity, SourceFile,
        SourceRange,
        api::rule::{Confidence, Matcher, Rule, Severity as RuleSeverity},
    };

    fn range(line: u32, start: u32, end: u32) -> SourceRange {
        SourceRange {
            start: Position {
                line,
                column: start,
            },
            end: Position { line, column: end },
        }
    }

    fn finding() -> Finding {
        Finding {
            rule_id: RuleId::parse("js:network.request").unwrap(),
            message_id: "detected".into(),
            message: "request detected".into(),
            severity: Severity::Warning,
            range: range(2, 4, 12),
            evidence: vec![
                Evidence {
                    message: "source".into(),
                    range: Some(range(1, 1, 3)),
                    source: Some("é".into()),
                },
                Evidence {
                    message: "context".into(),
                    range: None,
                    source: None,
                },
            ],
        }
    }

    #[test]
    fn qualifies_findings_and_preserves_missing_evidence_ranges() {
        let report = LintReport {
            schema_version: crate::REPORT_VERSION,
            tool_version: "test".into(),
            findings: vec![finding()],
            parse_diagnostics: Vec::new(),
        };
        let file = ProjectFileReport::from_lint_report("src/é.js", report);

        assert_eq!(file.path, "src/é.js");
        assert_eq!(file.findings[0].location.path, "src/é.js");
        assert_eq!(
            file.findings[0].evidence[0].location.as_ref().unwrap().path,
            "src/é.js"
        );
        assert!(file.findings[0].evidence[1].location.is_none());
        assert_eq!(file.findings[0].evidence[0].source.as_deref(), Some("é"));
    }

    #[test]
    fn assembles_empty_and_parse_only_files_with_shared_summary() {
        let parse_only = ProjectFileReport {
            path: "broken.js".into(),
            findings: Vec::new(),
            parse_diagnostics: vec![crate::ParseDiagnostic {
                code: "syntax".into(),
                message: "invalid syntax".into(),
                filename: "broken.js".into(),
                range: None,
            }],
        };
        let report = ProjectReport::from_file_reports(
            "test",
            vec![
                parse_only,
                ProjectFileReport {
                    path: "empty.js".into(),
                    findings: Vec::new(),
                    parse_diagnostics: Vec::new(),
                },
            ],
        );

        assert_eq!(report.summary().files, 2);
        assert_eq!(report.summary().parse_diagnostics, 1);
        assert_eq!(report.summary().findings, 0);
        assert_eq!(report.operations.files, 2);
        assert_eq!(report.operations.evidence, 0);
    }

    #[test]
    fn related_evidence_is_deduplicated_deterministically() {
        let mut project_finding = ProjectFinding::from_finding(finding(), "main.js");
        let related = ProjectEvidence {
            message: "related".into(),
            location: Some(SourceLocation {
                path: "dep.js".into(),
                range: range(3, 1, 2),
            }),
            source: None,
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
        let direct = ProjectFileReport::from_lint_report("main.js", linter.lint(source, "main.js"));
        let project = linter
            .lint_project(crate::ProjectInput {
                root: "/project".into(),
                sources: vec![SourceFile::new("main.js", source)],
                resolutions: Vec::new(),
            })
            .unwrap();

        assert_eq!(direct, project.files[0]);
    }
}
