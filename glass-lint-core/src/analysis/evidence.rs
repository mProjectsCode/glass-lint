//! Deterministic evidence annotation, bounding, grouping, and sorting.

use std::collections::BTreeMap;
use swc_common::Span;

use super::facts;
use crate::api::{
    classification::{ApiEvidence, ApiMatchKind},
    rule::ApiRule,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct EvidenceOccurrence {
    event: Option<facts::FactId>,
    span: Span,
    kind: ApiMatchKind,
    symbol_group: usize,
}

pub(super) struct AnnotatedEvidence {
    occurrences: Vec<EvidenceOccurrence>,
    symbols: Vec<String>,
}

pub(super) fn annotate(evidence: Vec<ApiEvidence>) -> AnnotatedEvidence {
    let mut symbols = Vec::with_capacity(evidence.len());
    let mut groups = BTreeMap::<(ApiMatchKind, String), usize>::new();
    let mut occurrences = Vec::new();
    for evidence in evidence {
        let ApiEvidence {
            kind,
            symbol,
            spans,
            event_ids,
            ..
        } = evidence;
        let symbol_group = if let Some(group) = groups.get(&(kind, symbol.clone())) {
            *group
        } else {
            let group = symbols.len();
            groups.insert((kind, symbol.clone()), group);
            symbols.push(symbol);
            group
        };
        occurrences.extend(
            spans
                .into_iter()
                .filter(|span| !span.is_dummy())
                .enumerate()
                .map(|(position, span)| EvidenceOccurrence {
                    event: event_ids
                        .get(position)
                        .and_then(|event| (*event != u32::MAX).then_some(facts::FactId(*event))),
                    span,
                    kind,
                    symbol_group,
                }),
        );
    }
    AnnotatedEvidence {
        occurrences,
        symbols,
    }
}

pub(super) fn normalize(mut annotated: AnnotatedEvidence) -> Vec<ApiEvidence> {
    let symbols = &annotated.symbols;
    let occurrences = &mut annotated.occurrences;
    occurrences.sort_by_key(|item| {
        (
            item.event.map(|event| event.0).unwrap_or(u32::MAX),
            item.span.lo,
            item.span.hi,
            item.kind,
            item.symbol_group,
        )
    });
    occurrences.dedup();
    occurrences.truncate(ApiRule::EVIDENCE_LIMIT);
    let mut grouped = BTreeMap::<(ApiMatchKind, usize), Vec<(Option<facts::FactId>, Span)>>::new();
    for occurrence in occurrences {
        grouped
            .entry((occurrence.kind, occurrence.symbol_group))
            .or_default()
            .push((occurrence.event, occurrence.span));
    }
    let mut normalized = grouped
        .into_iter()
        .map(|((kind, symbol_group), occurrences)| ApiEvidence {
            kind,
            symbol: symbols[symbol_group].clone(),
            count: u32::try_from(occurrences.len()).unwrap_or(u32::MAX),
            spans: occurrences.iter().map(|(_, span)| *span).collect(),
            event_ids: occurrences
                .iter()
                .map(|(event, _)| event.map_or(u32::MAX, |event| event.0))
                .collect(),
        })
        .collect::<Vec<_>>();
    normalized.sort_by_key(|item| {
        (
            item.spans.first().map(|span| (span.lo, span.hi)),
            item.kind,
            item.symbol.clone(),
        )
    });
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::BytePos;

    fn evidence(symbol: &str, spans: &[u32]) -> ApiEvidence {
        ApiEvidence {
            kind: ApiMatchKind::Call,
            symbol: symbol.into(),
            count: spans.len() as u32,
            spans: spans
                .iter()
                .map(|position| Span::new(BytePos(*position), BytePos(*position + 1)))
                .collect(),
            event_ids: spans.to_vec(),
        }
    }

    #[test]
    fn symbol_groups_preserve_order_and_merge_only_equal_symbols() {
        let normalized = normalize(annotate(vec![
            evidence("request", &[2, 4]),
            evidence("request", &[6]),
            evidence("other", &[8]),
        ]));
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].symbol, "request");
        assert_eq!(normalized[0].count, 3);
        assert_eq!(normalized[1].symbol, "other");
        assert_eq!(normalized[1].event_ids, vec![8]);
    }
}
