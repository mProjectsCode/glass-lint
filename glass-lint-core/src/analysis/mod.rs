//! Private per-file semantic and static analysis.

use std::collections::BTreeMap;
use swc_common::Span;
use swc_ecma_ast::Program;

use crate::api::{
    classification::{ApiEvidence, ApiMatchKind},
    rule::{ApiMatcher, ApiRule},
};

mod ast;
mod constant;
mod evidence_index;
mod fact_builder;
mod facts;
mod flow_calls;
mod flow_index;
mod flow_state;
mod object_flow;
mod resolution;
mod scope;
mod summary;
mod value;

use facts::SemanticFacts;

#[derive(Debug)]
pub(crate) struct SemanticModel<'matchers> {
    facts: SemanticFacts,
    matchers: Vec<&'matchers ApiMatcher>,
}

impl<'matchers> SemanticModel<'matchers> {
    pub(crate) fn analyze_compiled(
        program: &Program,
        matchers: &'matchers [&'matchers ApiMatcher],
        selected: &[usize],
    ) -> Self {
        let resolver = resolution::Resolver::collect(program);
        let facts = SemanticFacts::build(program, resolver, matchers, selected);
        Self {
            facts,
            matchers: matchers.to_vec(),
        }
    }

    pub(crate) fn evidence_for(&self, rule_index: usize) -> Vec<ApiEvidence> {
        if !self.facts.is_selected(rule_index) {
            return Vec::new();
        }
        let mut evidence = self.facts.index.evidence_for(self.matchers[rule_index]);
        evidence.extend_from_slice(&self.facts.argument_evidence[rule_index]);
        normalize_evidence(annotate_evidence(evidence))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct EvidenceOccurrence {
    event: Option<facts::FactId>,
    span: Span,
    kind: ApiMatchKind,
    symbol: String,
}

fn annotate_evidence(evidence: Vec<ApiEvidence>) -> Vec<EvidenceOccurrence> {
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

fn normalize_evidence(mut occurrences: Vec<EvidenceOccurrence>) -> Vec<ApiEvidence> {
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
