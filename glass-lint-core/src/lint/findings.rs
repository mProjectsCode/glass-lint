//! Conversion of semantic evidence into located lint findings.
//!
//! This layer converts the matcher-independent `ClassificationResult` into
//! rule-specific `Finding` values with stable source locations, evidence
//! items, and range containment checks. Range policy and report assembly
//! are kept separate from semantic fact construction.

use std::collections::BTreeMap;

use crate::{
    Evidence, EvidenceList, Finding, ProjectRelativePath, SourceLocation, SourceRange,
    api::classification::{ClassificationResult, MatchedCapability},
    diagnostic::SourceLineIndex,
    lint::{Linter, ranges::remove_contained_ranges},
};

impl Linter {
    /// Turn classifier capabilities into report findings. Classification is
    /// kept separate from report assembly so source-range policy remains in
    /// this layer and semantic analysis stays provider-neutral.
    pub(super) fn findings_for(
        &self,
        classification: &ClassificationResult,
        lines: &SourceLineIndex,
        path: &str,
    ) -> Vec<Finding> {
        classification
            .capabilities()
            .iter()
            .flat_map(|capability| self.findings_for_capability(capability, lines, path))
            .collect()
    }

    fn findings_for_capability(
        &self,
        capability: &MatchedCapability,
        lines: &SourceLineIndex,
        path: &str,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog().rule_id(capability.rule_index).cloned() else {
            return Vec::new();
        };

        // Build a map from SourceRange to evidence items, grouping by range.
        let mut by_range: BTreeMap<SourceRange, Evidence> = BTreeMap::new();
        for evidence in capability.evidence() {
            for occurrence in &evidence.occurrences {
                let span = occurrence.span;
                if span.is_empty() {
                    continue;
                }
                let Ok(range) = lines.try_range(span) else {
                    continue;
                };
                by_range.entry(range).or_insert_with(|| Evidence {
                    message: format!("{} of \"{}\"", evidence.kind().as_str(), evidence.symbol()),
                    count: evidence.count,
                    evidence_truncated: evidence.evidence_truncated,
                    location: None,
                });
            }
        }

        // Extract ranges, remove contained
        let mut ranges: Vec<SourceRange> = by_range.keys().cloned().collect();
        remove_contained_ranges(&mut ranges);

        // Build findings: group evidence by the containing range
        let path_owned = path.to_owned();
        ranges
            .into_iter()
            .map(|range| {
                let local_evidence: EvidenceList = by_range
                    .iter()
                    .filter(|(item_range, _)| range.contains(item_range))
                    .map(|(item_range, evidence)| {
                        let mut ev = evidence.clone();
                        ev.location = Some(SourceLocation {
                            path: ProjectRelativePath::from_normalized(path_owned.clone()),
                            range: item_range.clone(),
                        });
                        ev
                    })
                    .collect();
                Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: capability.label().into(),
                    severity: capability.severity(),
                    location: SourceLocation {
                        path: ProjectRelativePath::from_normalized(path_owned.clone()),
                        range,
                    },
                    evidence: local_evidence,
                }
            })
            .collect()
    }
}


