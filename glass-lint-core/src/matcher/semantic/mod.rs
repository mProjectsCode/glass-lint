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
mod index;
mod object_flow;
mod scope;

use index::MatcherFacts;

/// The matcher-oriented facts derived from one parsed JavaScript file.
///
/// Construction is deliberately private to the matcher module: callers supply
/// a parsed program and rules, then query the immutable facts.  The next
/// migration steps replace the internal compatibility collector with the
/// resolver/event implementation without changing this boundary.
#[derive(Debug)]
pub(super) struct SemanticModel {
    index: MatcherFacts,
    argument_evidence: Vec<Vec<ApiEvidence>>,
}

impl SemanticModel {
    pub(super) fn analyze(program: Option<&Program>, rules: &[ApiRule]) -> Self {
        let aliases = program.map(scope::ScopeGraph::collect).unwrap_or_default();
        let (index, argument_evidence) = MatcherFacts::collect_for_rules(program, &aliases, rules);
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
