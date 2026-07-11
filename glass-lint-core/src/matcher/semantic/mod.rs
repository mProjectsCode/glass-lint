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
    rule::ApiRule,
};

mod ast;
mod calls;
mod constant;
mod events;
mod index;
mod instance;
mod object_flow;
mod resolver;
mod scope;
mod value;

use index::MatcherFacts;

/// The matcher-oriented facts derived from one parsed JavaScript file.
///
/// Construction is deliberately private to the matcher module: callers supply
/// a parsed program and rules, then query immutable, rule-independent facts.
/// This keeps rule evaluation free of ad-hoc AST traversal and ensures every
/// matcher observes the same resolution decisions.
#[derive(Debug)]
pub(super) struct SemanticModel {
    index: MatcherFacts,
    argument_evidence: Vec<Vec<ApiEvidence>>,
}

impl SemanticModel {
    pub(super) fn analyze(program: &Program, rules: &[ApiRule]) -> Self {
        let resolver = resolver::Resolver::collect(program);
        // The event log is not matcher policy.  It is an invariant checked at
        // the analysis boundary so later position-sensitive consumers can rely
        // on one canonical source order.
        debug_assert!(resolver.events_are_source_ordered());
        let (index, argument_evidence) = MatcherFacts::collect_for_rules(program, &resolver, rules);
        Self {
            index,
            argument_evidence,
        }
    }

    pub(super) fn evidence_for(&self, rule_index: usize, rule: &ApiRule) -> Vec<ApiEvidence> {
        let mut evidence = self.index.evidence_for(rule);
        evidence.extend_from_slice(&self.argument_evidence[rule_index]);
        normalize_evidence(evidence)
    }
}

/// Normalize every semantic evidence path at the same boundary.  The input
/// order is deliberately irrelevant: spans are sorted first, identical
/// `(kind, symbol, span)` occurrences are collapsed, and the finite limit is
/// applied to source occurrences rather than matcher declaration order.
fn normalize_evidence(evidence: Vec<ApiEvidence>) -> Vec<ApiEvidence> {
    let mut occurrences = evidence
        .into_iter()
        .flat_map(|evidence| {
            evidence
                .spans
                .into_iter()
                .filter(|span| !span.is_dummy())
                .map(move |span| (span, evidence.kind, evidence.symbol.clone()))
        })
        .collect::<Vec<_>>();
    occurrences.sort_by(|left, right| {
        (left.0.lo, left.0.hi, left.1, &left.2).cmp(&(right.0.lo, right.0.hi, right.1, &right.2))
    });
    occurrences.dedup();
    occurrences.truncate(ApiRule::EVIDENCE_LIMIT);

    let mut grouped = BTreeMap::<(ApiMatchKind, String), Vec<Span>>::new();
    for (span, kind, symbol) in occurrences {
        grouped.entry((kind, symbol)).or_default().push(span);
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
