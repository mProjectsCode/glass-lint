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
mod call;
mod call_arguments;
mod calls;
mod constant;
mod constructors;
mod events;
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

use events::EventLog;
use facts::SemanticFacts;

/// The matcher-oriented facts derived from one parsed JavaScript file.
///
/// Construction is deliberately private to the matcher module: callers supply
/// a parsed program and rules, then query immutable, rule-independent facts.
/// This keeps rule evaluation free of ad-hoc AST traversal and ensures every
/// matcher observes the same resolution decisions.
#[derive(Debug)]
pub(super) struct SemanticModel {
    facts: SemanticFacts,
    matchers: Vec<ApiMatcher>,
}

impl SemanticModel {
    pub(super) fn analyze(program: &Program, rules: &[ApiRule]) -> Self {
        let matchers = rules
            .iter()
            .map(|rule| ApiMatcher::from_matchers(rule.matchers().to_vec()))
            .collect::<Vec<_>>();
        let resolver = resolver::Resolver::collect(program);
        let events = EventLog::collect(program)
            .with_scopes(|span| resolver.scope_chain_at(span).first().copied().unwrap_or(0));
        // The event log is the analysis boundary's source-order contract. A
        // malformed or overlarge event stream fails closed before any visitor
        // can manufacture a second ordering from the AST.
        if !events.is_source_ordered() {
            return Self {
                facts: SemanticFacts::empty(rules.len()),
                matchers,
            };
        }
        let facts = SemanticFacts::build(program, resolver, events, &matchers, rules.len());
        Self { facts, matchers }
    }

    pub(super) fn evidence_for(&self, rule_index: usize) -> Vec<ApiEvidence> {
        if !self.facts.events.is_source_ordered() {
            return Vec::new();
        }
        let mut evidence = self.facts.index.evidence_for(&self.matchers[rule_index]);
        evidence.extend_from_slice(&self.facts.argument_evidence[rule_index]);
        normalize_evidence(evidence, &self.facts.events)
    }
}

/// Normalize every semantic evidence path at the same boundary.  The input
/// order is deliberately irrelevant: spans are sorted first, identical
/// `(kind, symbol, span)` occurrences are collapsed, and the finite limit is
/// applied to source occurrences rather than matcher declaration order.
fn normalize_evidence(evidence: Vec<ApiEvidence>, events: &EventLog) -> Vec<ApiEvidence> {
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
        (
            events
                .order_for(left.0)
                .map(|event| event.0)
                .unwrap_or(u32::MAX),
            left.0.lo,
            left.0.hi,
            left.1,
            &left.2,
        )
            .cmp(&(
                events
                    .order_for(right.0)
                    .map(|event| event.0)
                    .unwrap_or(u32::MAX),
                right.0.lo,
                right.0.hi,
                right.1,
                &right.2,
            ))
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
