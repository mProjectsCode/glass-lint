//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::BTreeMap;

use glass_lint_datastructures::ByteRange;

use crate::{
    analysis::matching::occurrence::Occurrence,
    api::{
        classification::{ClassificationEvidence, ClassificationEvidenceOccurrence},
        rule::MatchKind,
    },
    diagnostic::SourceLineIndex,
    project::MatchCertainty,
};

/// A validated evidence group shared by the direct and constrained sinks.
pub(super) struct EvidenceGroup(ClassificationEvidence);

impl EvidenceGroup {
    pub(super) fn from_occurrences(
        kind: MatchKind,
        symbol: String,
        certainty: MatchCertainty,
        occurrences: impl IntoIterator<Item = Occurrence>,
    ) -> Option<Self> {
        let occurrences = occurrences
            .into_iter()
            .map(|occurrence| {
                ClassificationEvidenceOccurrence::new(
                    occurrence.span(),
                    Some(occurrence.event().raw()),
                    None,
                )
            })
            .collect();
        ClassificationEvidence::from_occurrences(kind, symbol, occurrences, certainty).map(Self)
    }

    /// Build a definite classification from occurrences, shared by the direct
    /// and constrained evidence-push paths.
    pub(super) fn definite_classification(
        kind: MatchKind,
        symbol: String,
        occurrences: impl IntoIterator<Item = Occurrence>,
    ) -> Option<ClassificationEvidence> {
        Self::from_occurrences(kind, symbol, MatchCertainty::Definite, occurrences)
            .map(Self::into_classification)
    }

    pub(super) fn into_classification(self) -> ClassificationEvidence {
        self.0
    }
}

/// Internal key that owns its data once and is used across all accumulators,
/// avoiding string clones for separate count and occurrence maps.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey(MatchKind, String);

/// Per-key accumulated state used during normalization.
struct RawEvidenceGroup {
    total_count: usize,
    certainty: crate::project::MatchCertainty,
    occurrences: Vec<crate::api::classification::ClassificationEvidenceOccurrence>,
}

#[derive(Default)]
struct EvidenceAccumulator {
    groups: BTreeMap<EvidenceKey, RawEvidenceGroup>,
}

impl EvidenceAccumulator {
    fn add(&mut self, item: &ClassificationEvidence) {
        let key = EvidenceKey(item.kind(), item.symbol().to_owned());
        let accum = self.groups.entry(key).or_insert_with(|| RawEvidenceGroup {
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
                ClassificationEvidence::from_parts(
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
            *group = ClassificationEvidence::from_parts(
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
mod tests;
