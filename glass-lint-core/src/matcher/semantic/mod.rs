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
    ) -> Self {
        Self::analyze_with_matchers(program, matchers, matchers.len())
    }

    fn analyze_with_matchers(
        program: &Program,
        matchers: &'matchers [&'matchers ApiMatcher],
        rule_count: usize,
    ) -> Self {
        let resolver = resolver::Resolver::collect(program);
        let facts = SemanticFacts::build(program, resolver, matchers, rule_count);
        Self {
            facts,
            matchers: matchers.to_vec(),
        }
    }

    pub(super) fn evidence_for(&self, rule_index: usize) -> Vec<ApiEvidence> {
        let mut evidence = self.facts.index.evidence_for(self.matchers[rule_index]);
        evidence.extend_from_slice(&self.facts.argument_evidence[rule_index]);
        normalize_evidence(annotate_evidence(evidence, &self.facts.stream))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct EvidenceOccurrence {
    event: Option<facts::FactId>,
    span: Span,
    kind: ApiMatchKind,
    symbol: String,
}

fn annotate_evidence(
    evidence: Vec<ApiEvidence>,
    facts: &facts::FactStream,
) -> Vec<EvidenceOccurrence> {
    evidence
        .into_iter()
        .flat_map(|evidence| {
            evidence
                .spans
                .into_iter()
                .filter(|span| !span.is_dummy())
                .map(move |span| EvidenceOccurrence {
                    event: facts.order_for_span(span),
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

    let mut grouped = BTreeMap::<(ApiMatchKind, String), Vec<Span>>::new();
    for occurrence in occurrences {
        grouped
            .entry((occurrence.kind, occurrence.symbol))
            .or_default()
            .push(occurrence.span);
    }
    let mut normalized = grouped
        .into_iter()
        .map(|((kind, symbol), spans)| ApiEvidence {
            kind,
            symbol,
            count: u32::try_from(spans.len()).unwrap_or(u32::MAX),
            spans,
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        let left_span = left.spans.first().map(|span| (span.lo, span.hi));
        let right_span = right.spans.first().map(|span| (span.lo, span.hi));
        (left_span, left.kind, &left.symbol).cmp(&(right_span, right.kind, &right.symbol))
    });
    normalized
}
