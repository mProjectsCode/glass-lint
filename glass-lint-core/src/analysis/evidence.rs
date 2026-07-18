//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::{BTreeMap, btree_map::Entry};

#[cfg(test)]
use crate::api::rule::Rule;
use crate::{
    ByteRange,
    analysis::facts::FactId,
    api::classification::{ClassificationEvidence, MatchKind, RelatedClassificationEvidence},
};

#[derive(Debug, PartialEq, Eq)]
/// One rule match retained until evidence is bounded and regrouped.
pub(super) struct EvidenceOccurrence {
    /// Fact identity used as the primary deterministic ordering key.
    event: Option<FactId>,
    /// Source location, including synthetic locations when no fact exists.
    span: ByteRange,
    /// Semantic match category shown in the public evidence.
    kind: MatchKind,
    /// Interned `(kind, symbol)` group used to merge equivalent entries.
    symbol_group: usize,
}

fn evidence_order(item: &EvidenceOccurrence) -> (u32, u32, u32, MatchKind, usize) {
    (
        item.span.start(),
        item.span.end(),
        item.event.map_or(u32::MAX, |event| event.0),
        item.kind,
        item.symbol_group,
    )
}

/// Intermediate evidence representation that separates sorting/bounding from
/// the public grouped shape and preserves related evidence by symbol group.
pub(super) struct AnnotatedEvidence {
    /// Occurrences before final sorting, deduplication, and bounding.
    occurrences: Vec<EvidenceOccurrence>,
    /// Symbols indexed by the compact group IDs in `occurrences`.
    symbols: Vec<String>,
    /// Related evidence retained for each symbol group.
    related: BTreeMap<usize, Vec<RelatedClassificationEvidence>>,
    total_counts: BTreeMap<usize, usize>,
    evidence_truncated: bool,
}

impl AnnotatedEvidence {
    /// Convert rule-local evidence into sortable occurrences while retaining
    /// the symbol table needed to reconstruct grouped output later.
    pub(super) fn from_evidence(evidence: Vec<ClassificationEvidence>, limit: usize) -> Self {
        let mut symbols = Vec::with_capacity(evidence.len());
        let mut groups = BTreeMap::<(MatchKind, String), usize>::new();
        let mut occurrences = Vec::new();
        let mut related = BTreeMap::new();
        let mut total_counts = BTreeMap::new();
        let mut evidence_truncated = false;
        for evidence in evidence {
            let ClassificationEvidence {
                kind,
                symbol,
                occurrences: source_occurrences,
                related: evidence_related,
                ..
            } = evidence;
            let symbol_group = match groups.entry((kind, symbol.clone())) {
                Entry::Occupied(group) => *group.get(),
                Entry::Vacant(entry) => {
                    let group = symbols.len();
                    entry.insert(group);
                    symbols.push(symbol);
                    group
                }
            };
            *total_counts.entry(symbol_group).or_default() += evidence.count as usize;
            related
                .entry(symbol_group)
                .or_insert_with(Vec::new)
                .extend(evidence_related);
            for occurrence in source_occurrences
                .into_iter()
                .filter(|occurrence| !occurrence.span.is_empty())
            {
                let occurrence = EvidenceOccurrence {
                    event: occurrence.fact.map(FactId),
                    span: occurrence.span,
                    kind,
                    symbol_group,
                };
                let index = occurrences
                    .binary_search_by(|candidate| {
                        evidence_order(candidate).cmp(&evidence_order(&occurrence))
                    })
                    .unwrap_or_else(|index| index);
                if occurrences
                    .get(index)
                    .is_some_and(|candidate| candidate == &occurrence)
                {
                    continue;
                }
                occurrences.insert(index, occurrence);
                if occurrences.len() > limit {
                    occurrences.pop();
                    evidence_truncated = true;
                }
            }
        }
        Self {
            occurrences,
            symbols,
            related,
            total_counts,
            evidence_truncated,
        }
    }

    /// Sort, bound, deduplicate, and regroup occurrences into public evidence.
    pub(super) fn into_evidence(self) -> Vec<ClassificationEvidence> {
        let mut grouped = BTreeMap::<(MatchKind, usize), Vec<(Option<FactId>, ByteRange)>>::new();
        for occurrence in &self.occurrences {
            grouped
                .entry((occurrence.kind, occurrence.symbol_group))
                .or_default()
                .push((occurrence.event, occurrence.span));
        }
        let mut normalized = grouped
            .into_iter()
            .map(
                |((kind, symbol_group), occurrences)| ClassificationEvidence {
                    kind,
                    symbol: self.symbols[symbol_group].clone(),
                    count: u32::try_from(
                        self.total_counts
                            .get(&symbol_group)
                            .copied()
                            .unwrap_or(occurrences.len()),
                    )
                    .unwrap_or(u32::MAX),
                    evidence_truncated: self.evidence_truncated,
                    occurrences: occurrences
                        .iter()
                        .map(|(event, span)| {
                            crate::api::classification::ClassificationEvidenceOccurrence {
                                span: *span,
                                fact: event.map(|event| event.0),
                            }
                        })
                        .collect(),
                    related: self.related.get(&symbol_group).cloned().unwrap_or_default(),
                },
            )
            .collect::<Vec<_>>();
        normalized.sort_by_key(|item| {
            (
                item.occurrences
                    .first()
                    .map(|occurrence| (occurrence.span.start(), occurrence.span.end())),
                item.kind,
                item.symbol.clone(),
            )
        });
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let normalized = AnnotatedEvidence::from_evidence(
            vec![
                evidence("request", &[2, 4]),
                evidence("request", &[6]),
                evidence("other", &[8]),
            ],
            Rule::EVIDENCE_LIMIT,
        )
        .into_evidence();
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].symbol, "request");
        assert_eq!(normalized[0].count, 3);
        assert_eq!(normalized[1].symbol, "other");
        assert_eq!(normalized[1].occurrences[0].fact, Some(8));
    }

    #[test]
    fn truncation_preserves_exact_count_and_marker() {
        let normalized = AnnotatedEvidence::from_evidence(
            vec![evidence(
                "request",
                &(0..(Rule::EVIDENCE_LIMIT + 4))
                    .map(|value| u32::try_from(value).unwrap() + 2)
                    .collect::<Vec<_>>(),
            )],
            Rule::EVIDENCE_LIMIT,
        )
        .into_evidence();
        assert_eq!(normalized[0].count as usize, Rule::EVIDENCE_LIMIT + 4);
        assert_eq!(normalized[0].occurrences.len(), Rule::EVIDENCE_LIMIT);
        assert!(normalized[0].evidence_truncated);
    }
}
