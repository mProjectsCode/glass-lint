//! Private per-file semantic and static analysis.

use swc_ecma_ast::Program;

use crate::Environment;
use crate::api::{classification::ApiEvidence, compiler::CompiledMatcherPlan};

mod evidence;
mod facts;
mod flow;
mod matching;
mod resolution;
mod scope;
mod syntax;
mod value;

use facts::SemanticFacts;

#[derive(Debug)]
pub(crate) struct SemanticModel<'matchers> {
    facts: SemanticFacts,
    matchers: Vec<&'matchers CompiledMatcherPlan>,
}

impl<'matchers> SemanticModel<'matchers> {
    pub(crate) fn analyze_compiled(
        program: &Program,
        matchers: &'matchers [&'matchers CompiledMatcherPlan],
        selected: &[usize],
        environment: &Environment,
    ) -> Self {
        let resolver = resolution::Resolver::collect_with_environment(program, environment);
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
        let mut evidence = self
            .facts
            .index
            .evidence_for(&self.matchers[rule_index].matcher);
        evidence.extend_from_slice(&self.facts.argument_evidence[rule_index]);
        evidence::normalize(evidence::annotate(evidence))
    }
}
