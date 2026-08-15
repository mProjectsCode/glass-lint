use std::collections::{BTreeMap, BTreeSet};

use hashbrown::HashMap;

use crate::{
    analysis::{
        facts::FactId,
        flow::{
            cross::state::{CallContext, CrossFlowState},
            effect::{EffectUse, FunctionEffect},
        },
        model::flow::FlowId,
        trace::{QualifiedEvent, TraceArena, TraceNodeId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind, RuleEvidenceTable, RuleIndex},
        compiler::CompiledObjectFlow,
    },
    project::{EvidenceRole, ModuleId},
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

impl EvidenceKey {
    fn for_call(flow: &CompiledObjectFlow, event: FactId) -> Self {
        Self {
            kind: MatchKind::CallArgument,
            symbol: flow.evidence_symbol().as_str().to_owned(),
            fact: event,
        }
    }
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
    rules: BTreeMap<RuleIndex, RuleEvidence>,
    pub(super) trace_heads: usize,
}

impl ModuleEvidence {
    pub(super) fn new(capacity: crate::api::classification::RuleEvidenceCapacity) -> Self {
        Self {
            capacity,
            rules: BTreeMap::new(),
            trace_heads: 0,
        }
    }

    fn rule_mut(&mut self, rule: RuleIndex) -> Option<&mut RuleEvidence> {
        (rule.get() < self.capacity.len()).then(|| self.rules.entry(rule).or_default())
    }

    fn rule(&self, rule: RuleIndex) -> Option<&RuleEvidence> {
        self.rules.get(&rule)
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
        for rule in self.rules.values_mut() {
            for item in rule.items.values_mut() {
                item.mark_possible();
            }
        }
    }

    pub(super) fn into_evidence(self) -> RuleEvidenceTable {
        let mut evidence = RuleEvidenceTable::new(self.capacity);
        for (rule_index, rule) in self.rules {
            if evidence
                .replace(rule_index, rule.items.into_values().collect())
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
    let key = EvidenceKey::for_call(flow, event);
    values.mark_nonmatching(rule_idx, &key);
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
    arena.intern_lifecycle_trace(
        source,
        requirements,
        prior_sinks,
        QualifiedEvent::new(module, event),
        EvidenceRole::Sink,
    )
}

pub(super) fn emit(
    session: &mut super::CrossProjectionSession<'_>,
    module: ModuleId,
    flow_id: FlowId,
    state: &CrossFlowState,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let Some(values) = session.evidence.get_mut(&module) else {
        return;
    };
    let rule_idx = flow_id.rule_index();
    let key = EvidenceKey::for_call(flow, event);
    let span = session
        .project
        .fact(QualifiedEvent::new(module, event))
        .map_or_else(glass_lint_datastructures::ByteRange::empty, |fact| {
            fact.span
        });

    // Trace construction keeps execution order and the terminal sink separate
    // from finding deduplication and certainty policy.
    let trace_head = assemble_trace(&mut *session.arena, state, module, event);

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
mod tests;
