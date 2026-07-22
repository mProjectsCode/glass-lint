//! Conversion of semantic evidence into located lint findings.
//!
//! This layer converts the matcher-independent `ClassificationResult` into
//! rule-specific `Finding` values with stable source locations, evidence
//! items, and range containment checks. Range policy and report assembly
//! are kept separate from semantic fact construction.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    Evidence, EvidenceList, Finding, ProjectRelativePath, SourceLocation, SourceRange,
    api::classification::MatchedCapability,
    diagnostic::SourceLineIndex,
    lint::{Linter, ranges::remove_contained_ranges},
};

impl Linter {
    pub(super) fn findings_for_capability(
        &self,
        capability: &MatchedCapability,
        lines: &SourceLineIndex,
        path: &str,
    ) -> Vec<Finding> {
        let Some(rule_id) = self.catalog().rule_id(capability.rule_index).cloned() else {
            return Vec::new();
        };

        let evidence_items = capability.evidence();
        if evidence_items.is_empty() {
            return Vec::new();
        }

        let mut by_range: BTreeMap<SourceRange, usize> = BTreeMap::new();
        for (ev_idx, evidence) in evidence_items.iter().enumerate() {
            for occurrence in &evidence.occurrences {
                let span = occurrence.span;
                if span.is_empty() {
                    continue;
                }
                let Ok(range) = lines.try_range(span) else {
                    continue;
                };
                by_range.entry(range).or_insert(ev_idx);
            }
        }

        let entries: Vec<(SourceRange, usize)> = by_range.into_iter().collect();

        let mut ranges: Vec<SourceRange> = entries.iter().map(|(r, _)| r.clone()).collect();
        remove_contained_ranges(&mut ranges);

        let path_shared: Arc<str> = Arc::from(path);
        let label: Arc<str> = Arc::from(capability.label());
        let severity = capability.severity();

        let mut groups: Vec<Vec<(usize, &SourceRange)>> = vec![Vec::new(); ranges.len()];
        let mut entry_cursor = 0usize;

        for (retained_idx, retained) in ranges.iter().enumerate() {
            while entry_cursor < entries.len()
                && entries[entry_cursor].0.end() < retained.start()
            {
                entry_cursor += 1;
            }

            let mut scan = entry_cursor;
            while scan < entries.len()
                && entries[scan].0.start() <= retained.end()
            {
                if retained.contains(&entries[scan].0) {
                    groups[retained_idx].push((entries[scan].1, &entries[scan].0));
                }
                scan += 1;
            }
        }

        ranges
            .into_iter()
            .enumerate()
            .map(|(retained_idx, range)| {
                let local_evidence: EvidenceList = groups[retained_idx]
                    .iter()
                    .map(|(ev_idx, item_range)| {
                        let ev = &evidence_items[*ev_idx];
                        Evidence {
                            message: format!("{} of \"{}\"", ev.kind().as_str(), ev.symbol()),
                            count: ev.count,
                            evidence_truncated: ev.evidence_truncated,
                            location: Some(SourceLocation {
                                path: ProjectRelativePath::from_normalized(Arc::clone(
                                    &path_shared,
                                )),
                                range: (*item_range).clone(),
                            }),
                        }
                    })
                    .collect();
                Finding {
                    rule_id: rule_id.clone(),
                    message_id: "detected".into(),
                    message: label.to_string(),
                    severity,
                    location: SourceLocation {
                        path: ProjectRelativePath::from_normalized(Arc::clone(&path_shared)),
                        range,
                    },
                    evidence: local_evidence,
                }
            })
            .collect()
    }
}
