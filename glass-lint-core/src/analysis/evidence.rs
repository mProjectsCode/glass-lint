//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::BTreeMap;

use crate::api::classification::{
    ClassificationEvidence, MatchKind, RelatedClassificationEvidence,
};
#[cfg(test)]
use crate::api::rule::Rule;

/// Internal key that owns its data once and is used across all accumulators,
/// avoiding string clones for separate count, related, and occurrence maps.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey(MatchKind, String);

/// Per-key accumulated state used during normalization.
struct EvidenceAccum {
    total_count: usize,
    related: Vec<RelatedClassificationEvidence>,
    occurrences: Vec<crate::api::classification::ClassificationEvidenceOccurrence>,
}

/// Sort, deduplicate, bound, and normalize evidence occurrences in place.
///
/// Within each `(kind, symbol)` group, occurrences are sorted by source
/// location, deduplicated, and truncated to `limit`. The `count` field
/// retains the original total (not the bounded count) so callers can report
/// how many events were found even when only a subset is shown.
pub(super) fn normalize_evidence(evidence: &mut Vec<ClassificationEvidence>, limit: usize) {
    let mut acc: BTreeMap<EvidenceKey, EvidenceAccum> = BTreeMap::new();

    for item in evidence.drain(..) {
        let key = EvidenceKey(item.kind, item.symbol);
        let accum = acc.entry(key).or_insert_with(|| EvidenceAccum {
            total_count: 0,
            related: Vec::new(),
            occurrences: Vec::new(),
        });
        accum.total_count = accum.total_count.saturating_add(item.count as usize);
        accum.related.extend(item.related);
        for occurrence in item.occurrences {
            if !occurrence.span.is_empty() {
                accum.occurrences.push(occurrence);
            }
        }
    }

    // Sort and deduplicate occurrences within each key.
    for (_, accum) in acc.iter_mut() {
        accum.occurrences.sort_by_key(|occurrence| {
            (
                occurrence.span.start(),
                occurrence.span.end(),
                occurrence.fact.unwrap_or(u32::MAX),
            )
        });
        accum.occurrences.dedup();
    }

    // Collect into a flat buffer maintaining the key for grouping.
    let mut flat: Vec<(EvidenceKey, crate::api::classification::ClassificationEvidenceOccurrence)> =
        Vec::new();
    for (key, accum) in &acc {
        for occurrence in &accum.occurrences {
            flat.push((key.clone(), occurrence.clone()));
        }
    }
    flat.sort_by_key(|(key, occurrence)| {
        (
            key.0,
            occurrence.span.start(),
            occurrence.span.end(),
            occurrence.fact.unwrap_or(u32::MAX),
        )
    });
    flat.dedup();

    let mut truncated = false;
    let mut grouped: BTreeMap<EvidenceKey, Vec<_>> = BTreeMap::new();
    for (key, occurrence) in flat {
        if grouped.len() < limit || grouped.contains_key(&key) {
            let entry = grouped.entry(key).or_default();
            if entry.len() < limit {
                entry.push(occurrence);
            } else {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }

    for (key, occurrences) in grouped {
        let total = acc
            .get(&key)
            .map(|a| a.total_count)
            .unwrap_or(occurrences.len());
        let related = acc
            .get_mut(&key)
            .map(|a| std::mem::take(&mut a.related))
            .unwrap_or_default();
        evidence.push(ClassificationEvidence {
            kind: key.0,
            symbol: key.1,
            count: u32::try_from(total).unwrap_or(u32::MAX),
            evidence_truncated: truncated,
            occurrences,
            related,
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
