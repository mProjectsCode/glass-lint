//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::BTreeMap;

use crate::api::classification::{
    ClassificationEvidence, MatchKind, RelatedClassificationEvidence,
};
#[cfg(test)]
use crate::api::rule::Rule;

/// Sort, deduplicate, bound, and normalize evidence occurrences in place.
///
/// Within each `(kind, symbol)` group, occurrences are sorted by source
/// location, deduplicated, and truncated to `limit`. The `count` field
/// retains the original total (not the bounded count) so callers can report
/// how many events were found even when only a subset is shown.
pub(super) fn normalize_evidence(evidence: &mut Vec<ClassificationEvidence>, limit: usize) {
    let mut all = Vec::new();
    let mut total_counts = BTreeMap::<(MatchKind, String), usize>::new();
    let mut related_map =
        BTreeMap::<(MatchKind, String), Vec<RelatedClassificationEvidence>>::new();

    for item in evidence.drain(..) {
        let key = (item.kind, item.symbol.clone());
        *total_counts.entry(key.clone()).or_default() += item.count as usize;
        related_map
            .entry(key.clone())
            .or_default()
            .extend(item.related);
        for occurrence in item.occurrences {
            if !occurrence.span.is_empty() {
                all.push((key.clone(), occurrence));
            }
        }
    }

    all.sort_by_key(|(key, occurrence)| {
        (
            key.0,
            occurrence.span.start(),
            occurrence.span.end(),
            occurrence.fact.unwrap_or(u32::MAX),
        )
    });
    all.dedup();

    let mut truncated = false;
    let mut grouped: BTreeMap<(MatchKind, String), Vec<_>> = BTreeMap::new();
    for (key, occurrence) in all {
        if grouped.len() < limit || grouped.contains_key(&key) {
            let entry = grouped.entry(key.clone()).or_default();
            if entry.len() < limit {
                entry.push(occurrence);
            } else {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }

    for ((kind, ref symbol), occurrences) in grouped {
        let total = total_counts
            .get(&(kind, symbol.clone()))
            .copied()
            .unwrap_or(occurrences.len());
        evidence.push(ClassificationEvidence {
            kind,
            symbol: symbol.clone(),
            count: u32::try_from(total).unwrap_or(u32::MAX),
            evidence_truncated: truncated,
            occurrences,
            related: related_map
                .remove(&(kind, symbol.clone()))
                .unwrap_or_default(),
        });
    }
    evidence.sort_by_key(|item| {
        (
            item.occurrences
                .first()
                .map(|o| (o.span.start(), o.span.end())),
            item.kind,
            item.symbol.clone(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ByteRange;

    fn evidence(symbol: &str, spans: &[u32]) -> ClassificationEvidence {
        ClassificationEvidence {
            kind: MatchKind::Call,
            symbol: symbol.into(),
            count: u32::try_from(spans.len()).unwrap_or(u32::MAX),
            evidence_truncated: false,
            occurrences: spans
                .iter()
                .map(
                    |position| crate::api::classification::ClassificationEvidenceOccurrence {
                        span: ByteRange::new(*position, *position + 1).unwrap(),
                        fact: Some(*position),
                    },
                )
                .collect(),
            related: Vec::new(),
        }
    }

    #[test]
    fn symbol_groups_preserve_order_and_merge_only_equal_symbols() {
        let mut evidence = vec![
            evidence("request", &[2, 4]),
            evidence("request", &[6]),
            evidence("other", &[8]),
        ];
        normalize_evidence(&mut evidence, Rule::EVIDENCE_LIMIT);
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].symbol, "request");
        assert_eq!(evidence[0].count, 3);
        assert_eq!(evidence[1].symbol, "other");
        assert_eq!(evidence[1].occurrences[0].fact, Some(8));
    }

    #[test]
    fn truncation_preserves_exact_count_and_marker() {
        let mut evidence = vec![evidence(
            "request",
            &(0..(Rule::EVIDENCE_LIMIT + 4))
                .map(|value| u32::try_from(value).unwrap() + 2)
                .collect::<Vec<_>>(),
        )];
        normalize_evidence(&mut evidence, Rule::EVIDENCE_LIMIT);
        assert_eq!(evidence[0].count as usize, Rule::EVIDENCE_LIMIT + 4);
        assert_eq!(evidence[0].occurrences.len(), Rule::EVIDENCE_LIMIT);
        assert!(evidence[0].evidence_truncated);
    }
}
