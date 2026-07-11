//! Per-file semantic facts consumed by declarative matchers.
//!
//! `SemanticModel` is intentionally the only entry point from the matcher
//! layer into JavaScript analysis.  Keeping the analysis result together
//! prevents rule evaluation from acquiring ad-hoc AST walks as new matcher
//! features are added.

use swc_ecma_ast::Program;

use super::{result::ApiEvidence, rule::ApiRule};

mod ast;
mod calls;
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
    pub(super) fn analyze(program: Option<&Program>, rules: &[ApiRule]) -> Self {
        let resolver = program.map(resolver::Resolver::collect).unwrap_or_default();
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
        evidence.truncate(ApiRule::EVIDENCE_LIMIT);
        evidence
    }
}
