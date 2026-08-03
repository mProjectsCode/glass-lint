use std::collections::BTreeSet;

use hashbrown::HashMap;

use crate::{
    analysis::{
        ProjectSemanticModel,
        facts::FactId,
        flow::{
            cross::state::{CallContext, CrossFlowState},
            effect::{EffectUse, FunctionEffect},
        },
        trace::{QualifiedEvent as TraceQualifiedEvent, TraceArena},
    },
    api::{
        classification::{
            ClassificationEvidence, MatchKind, RuleEvidenceTable, RuleIndex, TraceNodeId,
        },
        compiler::CompiledObjectFlow,
    },
    project::{EvidenceRole, ModuleId},
};

pub(super) fn effect_use_event(usage: &EffectUse) -> FactId {
    match usage {
        EffectUse::PropertyWrite { event, .. }
        | EffectUse::CallArgument { event, .. }
        | EffectUse::CallReceiver { event, .. } => *event,
    }
}

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
        } => {
            receiver.as_ref().is_some_and(|parameter| {
                context
                    .parameter
                    .is_some_and(|index| parameter.index() == index && parameter.is_root())
            }) || (context.parameter.is_none()
                && context.source_root.is_some_and(|root| {
                    effect
                        .value_root(*receiver_value)
                        .unwrap_or(*receiver_value)
                        == root
                }))
        }
        EffectUse::CallReceiver { receiver, .. } => context
            .parameter
            .is_some_and(|index| receiver.index() == index && receiver.is_root()),
        EffectUse::CallArgument {
            call_id,
            argument_index,
            ..
        } => effect
            .call_argument(*call_id, *argument_index)
            .is_some_and(|argument| {
                argument.parameter().is_some_and(|parameter| {
                    context
                        .parameter
                        .is_some_and(|index| parameter.index() == index && parameter.is_root())
                }) || (context.parameter.is_none()
                    && context.source_root.is_some_and(|root| {
                        effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value())
                            == root
                    }))
            }),
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
    items: Vec<ClassificationEvidence>,
    /// Sink occurrences reached by an alternative that did not produce a
    /// complete witness. These keys are kept separate from witness traces so
    /// no evidence can be assembled from incompatible call sites.
    nonmatching: BTreeSet<EvidenceKey>,
}

pub(super) struct ModuleEvidence {
    rules: Vec<RuleEvidence>,
    pub(super) trace_heads: usize,
}

impl ModuleEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            rules: (0..rule_count).map(|_| RuleEvidence::default()).collect(),
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
        if let Some(item) = rule.items.iter_mut().find(|item| {
            item.kind == key.kind
                && item.symbol == key.symbol
                && item
                    .occurrences
                    .iter()
                    .any(|occurrence| occurrence.fact == Some(key.fact.raw()))
        }) {
            item.certainty = crate::project::MatchCertainty::Possible;
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
            item.certainty = crate::project::MatchCertainty::Possible;
        }
        if let Some(existing) = rule.items.iter_mut().find(|existing| {
            existing.kind == key.kind
                && existing.symbol == key.symbol
                && existing
                    .occurrences
                    .iter()
                    .any(|occurrence| occurrence.fact == Some(key.fact.raw()))
        }) {
            existing.certainty = if existing.certainty == crate::project::MatchCertainty::Possible
                || item.certainty == crate::project::MatchCertainty::Possible
            {
                crate::project::MatchCertainty::Possible
            } else {
                crate::project::MatchCertainty::Definite
            };
            if !existing
                .occurrences
                .iter()
                .any(|occurrence| occurrence.trace == item.occurrences[0].trace)
            {
                existing.occurrences.append(&mut item.occurrences);
                existing.count = existing.count.saturating_add(item.count);
            }
        } else {
            rule.items.push(item);
        }
    }

    fn is_nonmatching(&self, rule: RuleIndex, key: &EvidenceKey) -> bool {
        self.rule(rule)
            .is_some_and(|rule| rule.nonmatching.contains(key))
    }

    pub(super) fn clear(&mut self) {
        for rule in &mut self.rules {
            rule.items.clear();
        }
    }

    pub(super) fn into_evidence(self) -> RuleEvidenceTable {
        let mut evidence = RuleEvidenceTable::new(self.rules.len());
        for (rule_index, rule) in self.rules.into_iter().enumerate() {
            evidence.replace(RuleIndex::new(rule_index), rule.items);
        }
        evidence
    }
}

pub(super) fn mark_nonmatching(
    evidence: &mut HashMap<ModuleId, ModuleEvidence>,
    module: ModuleId,
    flow_id: crate::analysis::model::flow::FlowId,
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
            symbol: flow.symbol.as_str().to_owned(),
            fact: event,
        },
    );
}

pub(super) struct EmissionContext<'a> {
    pub(super) project: &'a ProjectSemanticModel,
    pub(super) evidence: &'a mut HashMap<ModuleId, ModuleEvidence>,
    pub(super) arena: &'a mut TraceArena,
}

enum TraceBuild {
    Complete(TraceNodeId),
    Exhausted,
}

struct TraceAssembler<'a> {
    arena: &'a mut TraceArena,
    tail: TraceNodeId,
}

impl TraceAssembler<'_> {
    fn from_source<'a>(
        arena: &'a mut TraceArena,
        source: &crate::analysis::trace::QualifiedEvent,
    ) -> Option<TraceAssembler<'a>> {
        let tail = arena.intern(None, *source, EvidenceRole::Source)?;
        Some(TraceAssembler { arena, tail })
    }

    fn append(
        &mut self,
        event: &crate::analysis::trace::QualifiedEvent,
        role: EvidenceRole,
    ) -> bool {
        let Some(next) = self.arena.intern(Some(self.tail), *event, role) else {
            return false;
        };
        self.tail = next;
        true
    }

    fn finish_sink(self, module: ModuleId, event: FactId) -> Option<TraceNodeId> {
        self.arena.intern(
            Some(self.tail),
            TraceQualifiedEvent::new(module, event),
            EvidenceRole::Sink,
        )
    }
}

fn assemble_trace(
    arena: &mut TraceArena,
    state: &CrossFlowState,
    module: ModuleId,
    event: FactId,
) -> TraceBuild {
    let Some(source) = state.source() else {
        return TraceBuild::Exhausted;
    };
    let Some(mut trace) = TraceAssembler::from_source(arena, source) else {
        return TraceBuild::Exhausted;
    };
    for requirement in state.requirement_events() {
        if !trace.append(requirement, EvidenceRole::Requirement) {
            return TraceBuild::Exhausted;
        }
    }

    for sink in state.prior_sinks(module, event) {
        if !trace.append(&sink, EvidenceRole::Sink) {
            return TraceBuild::Exhausted;
        }
    }

    trace
        .finish_sink(module, event)
        .map_or(TraceBuild::Exhausted, TraceBuild::Complete)
}

pub(super) fn emit(
    context: EmissionContext<'_>,
    module: ModuleId,
    flow_id: crate::analysis::model::flow::FlowId,
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
        symbol: flow.symbol.as_str().to_owned(),
        fact: event,
    };
    let span = project
        .fact(module, event)
        .map_or_else(glass_lint_datastructures::ByteRange::empty, |fact| {
            fact.span
        });

    // Trace construction keeps execution order and the terminal sink separate
    // from finding deduplication and certainty policy.
    let trace_head = match assemble_trace(arena, state, module, event) {
        TraceBuild::Complete(head) => Some(head),
        TraceBuild::Exhausted => None,
    };

    let certainty = if values.is_nonmatching(rule_idx, &key) {
        crate::project::MatchCertainty::Possible
    } else {
        crate::project::MatchCertainty::Definite
    };
    let occurrence = crate::api::classification::ClassificationEvidenceOccurrence {
        span,
        fact: Some(event.raw()),
        trace: trace_head,
    };
    values.record(
        rule_idx,
        &key,
        ClassificationEvidence {
            kind: MatchKind::CallArgument,
            symbol: flow.evidence_symbol().as_str().to_owned(),
            count: 1,
            truncated: false,
            certainty,
            occurrences: vec![occurrence],
        },
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
        api::classification::RuleIndex,
    };

    #[test]
    fn trace_assembly_keeps_prior_sinks_as_sinks() {
        let mut state = CrossFlowState::known(
            FlowId::new(RuleIndex::new(0), 0),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(1)),
        );
        state.record_requirement(
            RequirementIndex::new(0),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(2)),
        );
        state.record_sink(
            SinkIndex::new(0),
            QualifiedEvent::new(ModuleId::new(1), FactId::new(3)),
        );
        let mut arena = TraceArena::new(10);
        let TraceBuild::Complete(head) =
            assemble_trace(&mut arena, &state, ModuleId::new(1), FactId::new(4))
        else {
            panic!("trace should fit within the arena");
        };
        let roles: Vec<_> = arena
            .reconstruct_trace(head)
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
}
