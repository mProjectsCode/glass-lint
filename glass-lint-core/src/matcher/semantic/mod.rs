//! Per-file semantic facts consumed by declarative matchers.
//!
//! `SemanticModel` is intentionally the only entry point from the matcher
//! layer into JavaScript analysis.  Keeping the analysis result together
//! prevents rule evaluation from acquiring ad-hoc AST walks as new matcher
//! features are added.

use swc_ecma_ast::Program;

use std::collections::BTreeMap;

use swc_common::Span;

use super::{
    result::{ApiEvidence, ApiMatchKind},
    rule::{ApiMatcher, ApiRule},
};

mod ast;
mod constant;
mod fact_builder;
mod facts;
mod flow_calls;
mod flow_index;
mod flow_state;
mod index;
mod object_flow;
mod resolver;
mod scope;
mod summary;
mod value;

use facts::SemanticFacts;

/// The matcher-oriented facts derived from one parsed JavaScript file.
///
/// Construction is deliberately private to the matcher module: callers supply
/// a parsed program and rules, then query immutable, rule-independent facts.
/// This keeps rule evaluation free of ad-hoc AST traversal and ensures every
/// matcher observes the same resolution decisions.
#[derive(Debug)]
pub(super) struct SemanticModel<'matchers> {
    facts: SemanticFacts,
    matchers: Vec<&'matchers ApiMatcher>,
}

impl<'matchers> SemanticModel<'matchers> {
    /// Analyze with pre-compiled, normalized matchers.  The `rule_count`
    /// is used to size per-rule evidence vectors.
    pub(super) fn analyze_compiled(
        program: &Program,
        matchers: &'matchers [&'matchers ApiMatcher],
        selected: &[usize],
    ) -> Self {
        let resolver = resolver::Resolver::collect(program);
        let facts = SemanticFacts::build(program, resolver, matchers, selected);
        Self {
            facts,
            matchers: matchers.to_vec(),
        }
    }

    pub(super) fn evidence_for(&self, rule_index: usize) -> Vec<ApiEvidence> {
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

/// Normalize every semantic evidence path at the same boundary.  The input
/// order is deliberately irrelevant: spans are sorted first, identical
/// `(kind, symbol, span)` occurrences are collapsed, and the finite limit is
/// applied to source occurrences rather than matcher declaration order.
fn normalize_evidence(mut occurrences: Vec<EvidenceOccurrence>) -> Vec<ApiEvidence> {
    occurrences.sort_by(|left, right| {
        (
            left.event.map(|event| event.0).unwrap_or(u32::MAX),
            left.span.lo,
            left.span.hi,
            left.kind,
            &left.symbol,
        )
            .cmp(&(
                right.event.map(|event| event.0).unwrap_or(u32::MAX),
                right.span.lo,
                right.span.hi,
                right.kind,
                &right.symbol,
            ))
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
        .map(|((kind, symbol), occurrences)| {
            let spans = occurrences
                .iter()
                .map(|(_, span)| *span)
                .collect::<Vec<_>>();
            let event_ids = occurrences
                .iter()
                .map(|(event, _)| event.map_or(u32::MAX, |event| event.0))
                .collect::<Vec<_>>();
            ApiEvidence {
                kind,
                symbol,
                count: u32::try_from(spans.len()).unwrap_or(u32::MAX),
                spans,
                event_ids,
            }
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        let left_span = left.spans.first().map(|span| (span.lo, span.hi));
        let right_span = right.spans.first().map(|span| (span.lo, span.hi));
        (left_span, left.kind, &left.symbol).cmp(&(right_span, right.kind, &right.symbol))
    });
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_normalization_keeps_originating_equal_span_events() {
        let span = Span::new(swc_common::BytePos(10), swc_common::BytePos(20));
        let evidence = normalize_evidence(vec![
            EvidenceOccurrence {
                event: Some(facts::FactId(2)),
                span,
                kind: ApiMatchKind::MemberCall,
                symbol: "obj.run".into(),
            },
            EvidenceOccurrence {
                event: Some(facts::FactId(1)),
                span,
                kind: ApiMatchKind::MemberCall,
                symbol: "obj.run".into(),
            },
        ]);
        assert_eq!(evidence[0].event_ids, vec![1, 2]);
        assert_eq!(evidence[0].spans, vec![span, span]);
    }

    #[test]
    fn downstream_projectors_do_not_depend_on_swc_or_program_nodes() {
        for source in [
            include_str!("index.rs"),
            include_str!("summary.rs"),
            include_str!("object_flow.rs"),
            include_str!("flow_index.rs"),
            include_str!("flow_calls.rs"),
            include_str!("flow_state.rs"),
        ] {
            assert!(!source.contains("swc_ecma_visit"));
            assert!(!source.contains("Program"));
        }
    }
}
