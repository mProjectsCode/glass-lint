//! Compiled matcher overlays and project-level matcher evidence.
//!
//! Projection is deliberately after local fact construction and project
//! linking. It applies qualified identities once, composes bounded flow, and
//! leaves rule selection to the compiled matcher catalog.

use std::{collections::BTreeMap, sync::Arc};

use super::super::{ModuleId, ProjectModule, ProjectSemanticModel, evidence, flow};
use crate::{
    analysis::{
        matching::{ModuleOccurrenceOverlay, OccurrenceIndexes},
        status::{AnalysisComponent, IncompleteReason},
    },
    api::{classification::ClassificationEvidence, compiler::CompiledRuleSelection},
};

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub struct ProjectMatcherModel<'matchers> {
    /// The immutable catalog used to select and query rules.
    matchers: CompiledRuleSelection<'matchers>,
    /// Per-module local indexes plus projected constrained/flow evidence.
    projections: BTreeMap<ModuleId, ProjectModuleProjection>,
}

#[derive(Debug)]
/// Materialized matcher inputs for one project module.
struct ProjectModuleProjection {
    /// Local occurrences after applying imported/namespace identities.
    index: Arc<OccurrenceIndexes>,
    overlay: ModuleOccurrenceOverlay,
    /// Direct constrained and cross-module flow evidence by rule index.
    projected: Vec<Vec<ClassificationEvidence>>,
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.
    pub fn project<'matchers>(
        &self,
        matchers: CompiledRuleSelection<'matchers>,
    ) -> ProjectMatcherModel<'matchers> {
        let projections: BTreeMap<ModuleId, ProjectModuleProjection> = self
            .modules
            .values()
            .map(|module| {
                let index = module.local().facts().shared_matcher_index();
                let identities = self.module_identities(module.id());
                let result_identities = self.call_result_identities(module.id());
                let overlay = index.module_overlay(&identities);
                (
                    module.id(),
                    ProjectModuleProjection {
                        index,
                        overlay,
                        projected: module.local().facts().project(
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
            self.status.borrow_mut().record(
                crate::analysis::status::StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Flow,
                    limit: self.flow_limit(),
                    observed: Some(projection_count),
                },
            );
        }
        self.effect_projections.set(projection_count);
        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                for (rule, values) in evidence.into_iter().enumerate() {
                    projection.projected[rule].extend(values);
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
        rule_index: crate::api::classification::RuleIndex,
        evidence_limit: usize,
    ) -> Vec<ClassificationEvidence> {
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
                projection
                    .index
                    .evidence_for_with_overlay(matcher.query(), Some(&projection.overlay))
            });
        if let Some(projected) = self
            .projections
            .get(&module.id())
            .and_then(|projection| projection.projected.get(rule_index.get()))
        {
            evidence.extend_from_slice(projected);
        }
        evidence::AnnotatedEvidence::from_evidence(evidence, evidence_limit).into_evidence()
    }
}
