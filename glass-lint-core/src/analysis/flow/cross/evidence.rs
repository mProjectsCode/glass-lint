use std::collections::{BTreeMap, BTreeSet};

use hashbrown::HashMap;

use crate::{
    analysis::{
        ProjectSemanticModel, facts::FactId, flow::{
            cross::state::{CallContext, CrossFlowState},
            effect::{EffectUse, FunctionEffect},
        }, model::flow::FlowId, trace::{QualifiedEvent, TraceArena, TraceNodeId, intern_lifecycle_trace},
    }, api::{
        classification::{ClassificationEvidence, MatchKind, RuleEvidenceTable, RuleIndex},
        compiler::CompiledObjectFlow,
    }, project::{EvidenceRole, ModuleId},
};

pub(super) fn usage_matches_context(
    effect: &FunctionEffect,
    usage: &EffectUse,
    context: &CallContext,
) -> bool {
    match usage {
        EffectUse::PropertyWrite {
            receiver,
            receiver_value,
            ..
        } => context.matches_property_write(effect, receiver.as_ref(), *receiver_value),
        EffectUse::CallReceiver { receiver, .. } => context.matches_call_receiver(receiver),
        EffectUse::CallArgument {
            call_id,
            argument_index,
            ..
        } => effect
            .call_argument(*call_id, *argument_index)
            .is_some_and(|argument| context.matches_argument(effect, argument)),
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey {
    kind: MatchKind,
    symbol: String,
    fact: FactId,
}

#[derive(Default)]
struct RuleEvidence {
    items: BTreeMap<EvidenceKey, ClassificationEvidence>,
    /// Sink occurrences reached by an alternative that did not produce a
    /// complete witness. These keys are kept separate from witness traces so
    /// no evidence can be assembled from incompatible call sites.
    nonmatching: BTreeSet<EvidenceKey>,
}

pub(super) struct ModuleEvidence {
    capacity: crate::api::classification::RuleEvidenceCapacity,
    rules: Vec<RuleEvidence>,
    pub(super) trace_heads: usize,
}

impl ModuleEvidence {
    pub(super) fn new(capacity: crate::api::classification::RuleEvidenceCapacity) -> Self {
        Self {
            capacity,
            rules: (0..capacity.len())
                .map(|_| RuleEvidence::default())
                .collect(),
            trace_heads: 0,
        }
    }

    fn rule_mut(&mut self, rule: RuleIndex) -> Option<&mut RuleEvidence> {
        self.rules.get_mut(rule.get())
    }

    fn rule(&self, rule: RuleIndex) -> Option<&RuleEvidence> {
        self.rules.get(rule.get())
    }

    fn mark_nonmatching(&mut self, rule_index: RuleIndex, key: &EvidenceKey) {
        let Some(rule) = self.rule_mut(rule_index) else {
            return;
        };
        rule.nonmatching.insert(key.clone());
        if let Some(item) = rule.items.get_mut(key) {
            item.mark_possible();
        }
    }

    fn record(
        &mut self,
        rule_index: RuleIndex,
        key: &EvidenceKey,
        mut item: ClassificationEvidence,
    ) {
        let Some(rule) = self.rule_mut(rule_index) else {
            return;
        };
        if rule.nonmatching.contains(key) {
            item.mark_possible();
        }
        if let Some(existing) = rule.items.get_mut(key) {
            let item_trace = item
                .occurrences()
                .first()
                .and_then(crate::api::classification::ClassificationEvidenceOccurrence::trace);
            if !existing
                .occurrences()
                .iter()
                .any(|occurrence| occurrence.trace() == item_trace)
            {
                existing.append(item);
            }
        } else {
            rule.items.insert(key.clone(), item);
        }
    }

    fn is_nonmatching(&self, rule: RuleIndex, key: &EvidenceKey) -> bool {
        self.rule(rule)
            .is_some_and(|rule| rule.nonmatching.contains(key))
    }

    pub(super) fn mark_all_possible(&mut self) {
        for rule in &mut self.rules {
            for item in rule.items.values_mut() {
                item.mark_possible();
            }
        }
    }

    pub(super) fn into_evidence(self) -> RuleEvidenceTable {
        let mut evidence = RuleEvidenceTable::new(self.capacity);
        for (rule_index, rule) in self.rules.into_iter().enumerate() {
            if evidence
                .replace(
                    RuleIndex::new(rule_index),
                    rule.items.into_values().collect(),
                )
                .is_err()
            {
                return evidence;
            }
        }
        evidence
    }
}

pub(super) fn mark_nonmatching(
    evidence: &mut HashMap<ModuleId, ModuleEvidence>,
    module: ModuleId,
    flow_id: FlowId,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let rule_idx = flow_id.rule_index();
    values.mark_nonmatching(
        rule_idx,
        &EvidenceKey {
            kind: MatchKind::CallArgument,
            symbol: flow.evidence_symbol().as_str().to_owned(),
            fact: event,
        },
    );
}

pub(super) struct EmissionContext<'a> {
    pub(super) project: &'a ProjectSemanticModel,
    pub(super) evidence: &'a mut HashMap<ModuleId, ModuleEvidence>,
    pub(super) arena: &'a mut TraceArena,
}

fn assemble_trace(
    arena: &mut TraceArena,
    state: &CrossFlowState,
    module: ModuleId,
    event: FactId,
) -> Option<TraceNodeId> {
    let source = state.source().copied()?;
    let requirements = state.requirement_events().copied();
    let prior_sinks = state.prior_sinks(module, event).into_iter();
    intern_lifecycle_trace(
        arena,
        source,
        requirements,
        prior_sinks,
        QualifiedEvent::new(module, event),
        EvidenceRole::Sink,
    )
}

pub(super) fn emit(
    context: EmissionContext<'_>,
    module: ModuleId,
    flow_id: FlowId,
    state: &CrossFlowState,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let EmissionContext {
        project,
        evidence,
        arena,
    } = context;
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let rule_idx = flow_id.rule_index();
    let key = EvidenceKey {
        kind: MatchKind::CallArgument,
        symbol: flow.evidence_symbol().as_str().to_owned(),
        fact: event,
    };
    let span = project
        .fact(QualifiedEvent::new(module, event))
        .map_or_else(glass_lint_datastructures::ByteRange::empty, |fact| {
            fact.span
        });

    // Trace construction keeps execution order and the terminal sink separate
    // from finding deduplication and certainty policy.
    let trace_head = assemble_trace(arena, state, module, event);

    let certainty = if values.is_nonmatching(rule_idx, &key) {
        crate::project::MatchCertainty::Possible
    } else {
        crate::project::MatchCertainty::Definite
    };
    let occurrence = crate::api::classification::ClassificationEvidenceOccurrence::new(
        span,
        Some(event.raw()),
        trace_head,
    );
    values.record(
        rule_idx,
        &key,
        ClassificationEvidence::from_occurrence(
            MatchKind::CallArgument,
            flow.evidence_symbol().as_str().to_owned(),
            occurrence,
            certainty,
        ),
    );
    if trace_head.is_some() {
        values.trace_heads = values.trace_heads.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis::{
            facts::FactId,
            flow::cross::state::QualifiedEvent,
            model::flow::{FlowId, RequirementIndex, SinkIndex},
        },
        api::classification::{ClassificationEvidenceOccurrence, RuleEvidenceCapacity, RuleIndex},
    };

    #[test]
    fn trace_assembly_keeps_prior_sinks_as_sinks() {
        let mut state = CrossFlowState::known(
            FlowId::new(RuleIndex::new(0), 0),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(1)),
        );
        state.record_requirement_for_test(
            RequirementIndex::new(0).unwrap(),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(2)),
        );
        state.record_sink_for_test(
            SinkIndex::new(0).unwrap(),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(3)),
        );
        let mut arena = TraceArena::new(10);
        let head = assemble_trace(&mut arena, &state, ModuleId::new(1), FactId::new(4))
            .expect("trace should fit within the arena");
        let roles: Vec<_> = arena
            .reconstruct_trace(head)
            .unwrap()
            .into_iter()
            .map(|step| step.role())
            .collect();
        assert_eq!(
            roles,
            vec![
                EvidenceRole::Source,
                EvidenceRole::Requirement,
                EvidenceRole::Sink,
                EvidenceRole::Sink,
            ]
        );
    }

    #[test]
    fn incomplete_projection_keeps_cross_evidence_as_possible() {
        let rule = RuleIndex::new(0);
        let key = EvidenceKey {
            kind: MatchKind::CallArgument,
            symbol: "fetch".to_owned(),
            fact: FactId::from_test(1),
        };
        let mut evidence = ModuleEvidence::new(RuleEvidenceCapacity::from_catalog_len(1));
        evidence.record(
            rule,
            &key,
            ClassificationEvidence::from_occurrence(
                MatchKind::CallArgument,
                "fetch".to_owned(),
                ClassificationEvidenceOccurrence::new(
                    glass_lint_datastructures::ByteRange::empty(),
                    Some(1),
                    None,
                ),
                crate::project::MatchCertainty::Definite,
            ),
        );

        evidence.mark_all_possible();

        let items = evidence.into_evidence();
        assert_eq!(
            items.for_rule(rule).unwrap()[0].certainty(),
            crate::project::MatchCertainty::Possible
        );
    }
}
