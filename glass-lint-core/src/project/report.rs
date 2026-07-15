//! Project finding assembly and deterministic evidence ownership.

use super::{ProjectEvidence, ProjectFinding, SourceLocation};

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
