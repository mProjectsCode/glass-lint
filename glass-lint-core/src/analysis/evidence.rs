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
    symbol: String,
}

pub(super) fn annotate(evidence: Vec<ApiEvidence>) -> Vec<EvidenceOccurrence> {
    evidence
        .into_iter()
        .flat_map(|evidence| {
            evidence
                .spans
                .into_iter()
                .filter(|span| !span.is_dummy())
                .enumerate()
                .map(move |(position, span)| EvidenceOccurrence {
                    event: evidence
                        .event_ids
                        .get(position)
                        .and_then(|event| (*event != u32::MAX).then_some(facts::FactId(*event))),
                    span,
                    kind: evidence.kind,
                    symbol: evidence.symbol.clone(),
                })
        })
        .collect()
}

pub(super) fn normalize(mut occurrences: Vec<EvidenceOccurrence>) -> Vec<ApiEvidence> {
    occurrences.sort_by_key(|item| {
        (
            item.event.map(|event| event.0).unwrap_or(u32::MAX),
            item.span.lo,
            item.span.hi,
            item.kind,
            item.symbol.clone(),
        )
    });
    occurrences.dedup();
    occurrences.truncate(ApiRule::EVIDENCE_LIMIT);
    let mut grouped = BTreeMap::<(ApiMatchKind, String), Vec<(Option<facts::FactId>, Span)>>::new();
    for occurrence in occurrences {
        grouped
            .entry((occurrence.kind, occurrence.symbol))
            .or_default()
            .push((occurrence.event, occurrence.span));
    }
    let mut normalized = grouped
        .into_iter()
        .map(|((kind, symbol), occurrences)| ApiEvidence {
            kind,
            symbol,
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
