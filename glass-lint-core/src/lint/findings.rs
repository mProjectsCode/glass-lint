//! Conversion of semantic evidence into located lint findings.

use super::{Linter, ranges::remove_contained_ranges};
use crate::{
    Evidence, Finding, ProjectRelativePath, SourceLocation,
    api::classification::{ClassificationEvidence, ClassificationResult, MatchedCapability},
    diagnostic::{SourceLineIndex, SourceRange},
};

impl Linter {
    /// Turn classifier capabilities into report findings. Classification is
    /// kept separate from report assembly so source-range policy remains in
    /// this layer and semantic analysis stays provider-neutral.
    pub(super) fn findings_for(
        &self,
        classification: &ClassificationResult,
        lines: &SourceLineIndex,
        source: &str,
        path: &str,
    ) -> Vec<Finding> {
        classification
            .capabilities()
            .iter()
            .flat_map(|capability| self.findings_for_capability(capability, lines, source, path))
            .collect()
    }

    fn findings_for_capability(
        &self,
        capability: &MatchedCapability,
        lines: &SourceLineIndex,
        source: &str,
        path: &str,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog().rule_id(capability.rule_index).cloned() else {
            return Vec::new();
        };
        let evidence: Vec<_> = capability
            .evidence()
            .iter()
            .flat_map(|evidence| {
                evidence
                    .occurrences
                    .iter()
                    .map(|occurrence| occurrence.span)
                    .filter(|span| !span.is_empty())
                    .filter_map(|span| Self::report_evidence(evidence, span, lines, source, path))
            })
            .collect();

        let mut ranges: Vec<_> = evidence
            .iter()
            .filter_map(|evidence| {
                evidence
                    .location
                    .as_ref()
                    .map(|location| location.range.clone())
            })
            .collect();
        remove_contained_ranges(&mut ranges);

        ranges
            .into_iter()
            .map(|range| {
                let local_evidence = evidence
                    .iter()
                    .filter(|evidence| {
                        evidence
                            .location
                            .as_ref()
                            .is_some_and(|location| contains_range(&range, &location.range))
                    })
                    .cloned()
                    .collect();

                Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: capability.label().into(),
                    severity: capability.severity(),
                    location: SourceLocation {
                        path: ProjectRelativePath::from_normalized(path.to_owned()),
                        range,
                    },
                    evidence: local_evidence,
                }
            })
            .collect()
    }

    fn report_evidence(
        evidence: &ClassificationEvidence,
        span: crate::ByteRange,
        lines: &SourceLineIndex,
        source: &str,
        path: &str,
    ) -> Option<Evidence> {
        let range = lines.try_range(source, span).ok()?;
        Some(Evidence {
            message: format!("{} of \"{}\"", evidence.kind().as_str(), evidence.symbol()),
            count: evidence.count,
            evidence_truncated: evidence.evidence_truncated,
            location: Some(SourceLocation {
                path: ProjectRelativePath::from_normalized(path.to_owned()),
                range,
            }),
        })
    }
}

/// Test finding/evidence containment using source-range ordering.
pub fn contains_range(outer: &SourceRange, inner: &SourceRange) -> bool {
    outer.contains(inner)
}
