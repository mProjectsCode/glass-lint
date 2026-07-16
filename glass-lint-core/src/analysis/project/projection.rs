//! ApiMatcher overlays and project-level matcher evidence.

use std::collections::BTreeMap;

use super::super::{ModuleId, ProjectModule, ProjectSemanticModel, evidence, flow};
use crate::{
    analysis::matching::MatcherFacts,
    api::{classification::ApiEvidence, compiler::CompiledMatcherCatalog},
};

#[derive(Debug)]
pub struct ProjectMatcherModel<'matchers> {
    matchers: CompiledMatcherCatalog<'matchers>,
    projections: BTreeMap<ModuleId, ProjectModuleProjection>,
}

#[derive(Debug)]
struct ProjectModuleProjection {
    index: MatcherFacts,
    arguments: Vec<Vec<ApiEvidence>>,
}

impl ProjectSemanticModel {
    pub fn project<'matchers>(
        &self,
        matchers: CompiledMatcherCatalog<'matchers>,
    ) -> ProjectMatcherModel<'matchers> {
        let projections: BTreeMap<ModuleId, ProjectModuleProjection> = self
            .modules
            .values()
            .map(|module| {
                let mut facts = module.local().facts().cloned_matcher_facts();
                let identities = self.module_identities(module.id());
                let result_identities = self.call_result_identities(module.id());
                facts.apply_module_overlay(&identities);
                (
                    module.id(),
                    ProjectModuleProjection {
                        index: facts,
                        arguments: module.local().facts().project(
                            &matchers,
                            Some(&identities),
                            Some(&result_identities),
                        ),
                    },
                )
            })
            .collect();
        let (cross, exhausted, projection_count) = flow::cross::collect(self, &matchers);
        if exhausted {
            self.flow_budget.mark_exhausted();
        }
        self.effect_projections.set(projection_count);
        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                for (rule, values) in evidence.into_iter().enumerate() {
                    projection.arguments[rule].extend(values);
                }
            }
        }
        ProjectMatcherModel {
            matchers,
            projections,
        }
    }
}

impl ProjectMatcherModel<'_> {
    pub fn evidence_for(&self, module: &ProjectModule, rule_index: usize) -> Vec<ApiEvidence> {
        if !self.matchers.is_selected(rule_index) {
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get(rule_index) else {
            return Vec::new();
        };
        let mut evidence = self
            .projections
            .get(&module.id())
            .map_or_else(Vec::new, |projection| {
                projection.index.evidence_for(&matcher.matcher)
            });
        if let Some(projected) = self
            .projections
            .get(&module.id())
            .and_then(|projection| projection.arguments.get(rule_index))
        {
            evidence.extend_from_slice(projected);
        }
        evidence::AnnotatedEvidence::from_evidence(evidence).into_evidence()
    }
}
