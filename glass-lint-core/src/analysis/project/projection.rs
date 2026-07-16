//! ApiMatcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::collections::BTreeMap;

use super::super::{ModuleId, ProjectModule, ProjectSemanticModel, evidence, flow};
use crate::{
    analysis::matching::MatcherFacts,
    api::{classification::ApiEvidence, compiler::CompiledMatcherCatalog},
};

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub struct ProjectMatcherModel<'matchers> {
    /// The immutable catalog used to select and query rules.
    matchers: CompiledMatcherCatalog<'matchers>,
    /// Per-module local indexes plus projected argument evidence.
    projections: BTreeMap<ModuleId, ProjectModuleProjection>,
}

#[derive(Debug)]
/// Materialized matcher inputs for one project module.
struct ProjectModuleProjection {
    /// Local occurrences after applying imported/namespace identities.
    index: MatcherFacts,
    /// Direct and cross-module argument evidence grouped by rule index.
    arguments: Vec<Vec<ApiEvidence>>,
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.
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
    /// Return deterministic, deduplicated evidence for a selected rule.
    pub fn evidence_for(
        &self,
        module: &ProjectModule,
        rule_index: usize,
        evidence_limit: usize,
    ) -> Vec<ApiEvidence> {
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
        evidence::AnnotatedEvidence::from_evidence(evidence, evidence_limit).into_evidence()
    }
}
