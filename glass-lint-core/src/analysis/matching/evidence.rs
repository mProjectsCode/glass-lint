//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::BTreeMap;

use glass_lint_datastructures::ByteRange;

#[cfg(test)]
use crate::api::rule::Rule;
use crate::{
    api::classification::{ClassificationEvidence, MatchKind},
    diagnostic::SourceLineIndex,
};

/// Internal key that owns its data once and is used across all accumulators,
/// avoiding string clones for separate count and occurrence maps.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey(MatchKind, String);

/// Per-key accumulated state used during normalization.
struct EvidenceAccum {
    total_count: usize,
    certainty: crate::project::MatchCertainty,
    occurrences: Vec<crate::api::classification::ClassificationEvidenceOccurrence>,
}

#[derive(Default)]
struct EvidenceAccumulator {
    groups: BTreeMap<EvidenceKey, EvidenceAccum>,
}

impl EvidenceAccumulator {
    fn add(&mut self, item: &ClassificationEvidence) {
        let key = EvidenceKey(item.kind(), item.symbol().to_owned());
        let accum = self.groups.entry(key).or_insert_with(|| EvidenceAccum {
            total_count: 0,
            certainty: crate::project::MatchCertainty::Definite,
            occurrences: Vec::new(),
        });
        if item.certainty() == crate::project::MatchCertainty::Possible {
            accum.certainty = crate::project::MatchCertainty::Possible;
        }
        accum.total_count = accum.total_count.saturating_add(item.count() as usize);
        accum.occurrences.extend(
            item.occurrences()
                .iter()
                .copied()
                .filter(|occurrence| !occurrence.span().is_empty()),
        );
    }

    fn finish(self) -> Vec<ClassificationEvidence> {
        self.groups
            .into_iter()
            .map(|(key, mut accum)| {
                accum.occurrences.sort_by_key(|occurrence| {
                    (
                        occurrence.span().start(),
                        occurrence.span().end(),
                        occurrence.fact().unwrap_or(u32::MAX),
                    )
                });
                accum.occurrences.dedup_by(|a, b| {
                    a.span() == b.span() && a.fact() == b.fact() && a.trace() == b.trace()
                });
                ClassificationEvidence::with_total_count(
                    key.0,
                    key.1,
                    accum.total_count,
                    false,
                    accum.certainty,
                    accum.occurrences,
                )
                .expect("evidence totals include every retained occurrence")
            })
            .collect()
    }
}

struct EvidencePresenter {
    limit: usize,
}

impl EvidencePresenter {
    fn present(&self, mut groups: Vec<ClassificationEvidence>) -> Vec<ClassificationEvidence> {
        for group in &mut groups {
            if group.occurrences().len() <= self.limit {
                continue;
            }
            let occurrences = group
                .occurrences()
                .iter()
                .copied()
                .take(self.limit)
                .collect();
            *group = ClassificationEvidence::with_total_count(
                group.kind(),
                group.symbol().to_owned(),
                group.count() as usize,
                true,
                group.certainty(),
                occurrences,
            )
            .expect("bounded evidence retains its original total count");
        }

        groups.sort_by(|left, right| {
            let left_span = left
                .occurrences()
                .first()
                .map(|occurrence| (occurrence.span().start(), occurrence.span().end()));
            let right_span = right
                .occurrences()
                .first()
                .map(|occurrence| (occurrence.span().start(), occurrence.span().end()));
            (left_span, left.kind(), left.symbol()).cmp(&(right_span, right.kind(), right.symbol()))
        });

        let global_truncated = groups.len() > self.limit;
        if global_truncated {
            groups.truncate(self.limit);
            for group in &mut groups {
                group.mark_truncated();
            }
        }
        groups
    }
}

/// Narrow an evidence location to the text selected by its matcher.
///
/// The report layer only groups and serializes the resulting span. Matcher
/// families own the source-specific strategy, including boundary-aware
/// private-network matching.
pub fn display_span(
    lines: &SourceLineIndex,
    span: ByteRange,
    kind: MatchKind,
    symbol: &str,
) -> ByteRange {
    if kind != MatchKind::StringContains {
        return span;
    }
    let Some(source) = lines.source_slice(span) else {
        return span;
    };
    let relative = if symbol == crate::api::rule::query::PRIVATE_NETWORK_EVIDENCE_SYMBOL {
        super::query::private_network_match(source)
    } else {
        source
            .find(symbol)
            .map(|start| (start, start.saturating_add(symbol.len())))
    };
    let Some((start, end)) = relative else {
        return span;
    };
    let Ok(start) = u32::try_from(start) else {
        return span;
    };
    let Ok(end) = u32::try_from(end) else {
        return span;
    };
    let Some(absolute_start) = span.start().checked_add(start) else {
        return span;
    };
    let Some(absolute_end) = span.start().checked_add(end) else {
        return span;
    };
    ByteRange::new(absolute_start, absolute_end).unwrap_or(span)
}

/// Sort, deduplicate, bound, and normalize evidence occurrences in place.
///
/// Within each `(kind, symbol)` group, occurrences are sorted and deduplicated.
/// The `count` field retains the original total so callers can report how many
/// events were found even when only a subset is shown.  Truncation applies
/// both per group and to the total number of groups.
pub(in crate::analysis) fn normalize_evidence(
    evidence: &mut Vec<ClassificationEvidence>,
    limit: usize,
) {
    let mut accumulator = EvidenceAccumulator::default();
    for item in evidence.drain(..) {
        accumulator.add(&item);
    }
    *evidence = EvidencePresenter { limit }.present(accumulator.finish());
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::ByteRange;

    use super::*;

    fn evidence(symbol: &str, spans: &[u32]) -> ClassificationEvidence {
        ClassificationEvidence::from_occurrences(
            MatchKind::Call,
            symbol.into(),
            spans
                .iter()
                .map(|position| {
                    crate::api::classification::ClassificationEvidenceOccurrence::new(
                        ByteRange::new(*position, *position + 1).unwrap(),
                        Some(*position),
                        None,
                    )
                })
                .collect(),
            crate::project::MatchCertainty::Definite,
        )
        .unwrap()
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
        assert_eq!(evidence[0].symbol(), "request");
        assert_eq!(evidence[0].count(), 3);
        assert_eq!(evidence[1].symbol(), "other");
        assert_eq!(evidence[1].occurrences()[0].fact(), Some(8));
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
        assert_eq!(evidence[0].count() as usize, Rule::EVIDENCE_LIMIT + 4);
        assert_eq!(evidence[0].occurrences().len(), Rule::EVIDENCE_LIMIT);
        assert!(evidence[0].is_truncated());
    }
}
