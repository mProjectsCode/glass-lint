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
    occurrences_truncated: bool,
    related: Vec<RelatedClassificationEvidence>,
    occurrences: Vec<crate::api::classification::ClassificationEvidenceOccurrence>,
}

/// Sort, deduplicate, bound, and normalize evidence occurrences in place.
///
/// Within each `(kind, symbol)` group, occurrences are sorted and deduplicated.
/// The `count` field retains the original total so callers can report how many
/// events were found even when only a subset is shown.  Truncation applies
/// both per group and to the total number of groups.
pub(super) fn normalize_evidence(evidence: &mut Vec<ClassificationEvidence>, limit: usize) {
    let mut acc: BTreeMap<EvidenceKey, EvidenceAccum> = BTreeMap::new();

    for item in evidence.drain(..) {
        let key = EvidenceKey(item.kind, item.symbol);
        let accum = acc.entry(key).or_insert_with(|| EvidenceAccum {
            total_count: 0,
            occurrences_truncated: false,
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

    // Sort and deduplicate occurrences within each key, then apply the
    // per-group occurrence limit directly so caller-owned storage is reused.
    for accum in acc.values_mut() {
        accum.occurrences.sort_by_key(|occurrence| {
            (
                occurrence.span.start(),
                occurrence.span.end(),
                occurrence.fact.unwrap_or(u32::MAX),
            )
        });
        accum.occurrences.dedup();
        if accum.occurrences.len() > limit {
            accum.occurrences.truncate(limit);
            accum.occurrences_truncated = true;
        }
    }

    // Build evidence items sorted by (first_span, kind, symbol) so the
    // global group limit selects the earliest groups in a stable order.
    // This replaces the old flat-vec / rebuild cycle that cloned every
    // string-bearing key for each occurrence and then looked back into the
    // accumulator map.
    let mut sorted: Vec<ClassificationEvidence> = acc
        .into_iter()
        .map(|(key, accum)| ClassificationEvidence {
            kind: key.0,
            symbol: key.1,
            count: u32::try_from(accum.total_count).unwrap_or(u32::MAX),
            truncated: accum.occurrences_truncated,
            occurrences: accum.occurrences,
            related: accum.related,
        })
        .collect();
    sorted.sort_by(|left, right| {
        let left_span = left
            .occurrences
            .first()
            .map(|occurrence| (occurrence.span.start(), occurrence.span.end()));
        let right_span = right
            .occurrences
            .first()
            .map(|occurrence| (occurrence.span.start(), occurrence.span.end()));
        (left_span, left.kind, left.symbol.as_str()).cmp(&(
            right_span,
            right.kind,
            right.symbol.as_str(),
        ))
    });

    // Apply the global group limit.
    let global_truncated = if sorted.len() > limit {
        sorted.truncate(limit);
        true
    } else {
        false
    };
    if global_truncated {
        for item in &mut sorted {
            item.truncated = true;
        }
    }

    *evidence = sorted;
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
            truncated: false,
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
        assert!(evidence[0].truncated);
    }
}
