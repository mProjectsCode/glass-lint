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
        status::{AnalysisComponent, IncompleteReason, StatusScope},
    },
    api::{
        classification::{ClassificationEvidence, RuleIndex},
        compiler::CompiledRuleSelection,
    },
};

#[derive(Debug)]
/// Matcher-independent facts and cross-file evidence for one linked project.
pub struct ProjectMatcherModel<'matchers> {
    matchers: CompiledRuleSelection<'matchers>,
    projections: BTreeMap<ModuleId, ProjectModuleProjection>,
}

#[derive(Debug)]
struct ProjectModuleProjection {
    index: Arc<OccurrenceIndexes>,
    overlay: ModuleOccurrenceOverlay,
    projected: Vec<Vec<ClassificationEvidence>>,
}

/// Side effects produced by a projection that were previously written back
/// into the project model through hidden interior mutability.  The caller
/// decides how to merge or report these instead of the project mutating
/// itself through a shared reference.
#[derive(Debug, Default)]
pub struct ProjectionOutcome {
    /// Whether cross-module flow projection exhausted its budget.
    pub flow_exhausted: bool,
    /// Number of effect projections performed during this projection.
    pub effect_projections: usize,
    /// Operation count when exhaustion was reached, if applicable.
    pub flow_observed: Option<usize>,
}

impl ProjectSemanticModel {
    /// Project a linked semantic model into matcher queries without rewalking
    /// any source AST.  Side effects such as budget exhaustion and projection
    /// counts are returned in a `ProjectionOutcome` instead of being written
    /// back into `self`.
    pub fn project<'matchers>(
        &self,
        matchers: CompiledRuleSelection<'matchers>,
    ) -> (ProjectMatcherModel<'matchers>, ProjectionOutcome) {
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
                            module.local().effects(),
                            &matchers,
                            Some(&identities),
                            Some(&result_identities),
                        ),
                    },
                )
            })
            .collect();

        let (cross, exhausted, projection_count) = flow::cross::collect(self, &matchers);
        let outcome = ProjectionOutcome {
            flow_exhausted: exhausted,
            effect_projections: projection_count,
            flow_observed: exhausted.then_some(projection_count),
        };

        let mut projections = projections;
        for (module, evidence) in cross {
            if let Some(projection) = projections.get_mut(&module) {
                for (rule, values) in evidence.into_iter().enumerate() {
                    projection.projected[rule].extend(values);
                }
            }
        }

        (
            ProjectMatcherModel {
                matchers,
                projections,
            },
            outcome,
        )
    }

    /// Merge projection side effects back into this project model.
    ///
    /// This exists so that existing callers that read `operation_counts` or
    /// `is_complete` after classification continue to see consistent values
    /// without requiring a full refactor of every call site.  New callers
    /// should prefer to consume the `ProjectionOutcome` directly.
    pub(crate) fn merge_projection_outcome(&self, outcome: &ProjectionOutcome) {
        if outcome.flow_exhausted {
            self.flow_budget.mark_exhausted();
            self.status.borrow_mut().record(
                StatusScope::Project,
                IncompleteReason::BudgetExhausted {
                    component: AnalysisComponent::Flow,
                    limit: self.flow_limit(),
                    observed: outcome.flow_observed,
                },
            );
        }
        self.effect_projections.set(outcome.effect_projections);
    }
}

impl ProjectMatcherModel<'_> {
    /// Return deterministic, deduplicated evidence for a selected rule.
    pub fn evidence_for(
        &self,
        module: &ProjectModule,
        rule_index: RuleIndex,
        evidence_limit: usize,
    ) -> Vec<ClassificationEvidence> {
        if !self.matchers.is_selected(rule_index) {
            return Vec::new();
        }
        let Some(matcher) = self.matchers.get(rule_index) else {
            return Vec::new();
        };
        let Some(names) = module.local().facts().names() else {
            return Vec::new();
        };
        let mut evidence = self
            .projections
            .get(&module.id())
            .map_or_else(Vec::new, |projection| {
                projection.index.evidence_for_with_overlay(
                    matcher.query(),
                    Some(&projection.overlay),
                    names,
                )
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
