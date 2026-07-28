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
        classification::{ClassificationEvidence, MatchKind},
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

pub(super) struct ModuleEvidence {
    pub(super) evidence: Vec<Vec<ClassificationEvidence>>,
    pub(super) seen: Vec<BTreeSet<(MatchKind, String, u32)>>,
}

impl ModuleEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            evidence: vec![Vec::new(); rule_count],
            seen: vec![BTreeSet::new(); rule_count],
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    project: &ProjectSemanticModel,
    evidence: &mut HashMap<ModuleId, ModuleEvidence>,
    module: ModuleId,
    flow_id: crate::analysis::model::flow::FlowId,
    state: &CrossFlowState,
    event: FactId,
    flow: &CompiledObjectFlow,
    arena: &mut TraceArena,
) {
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let rule_idx = flow_id.rule_index().get();
    if !values.seen[rule_idx].insert((MatchKind::CallArgument, flow.symbol.clone(), event.0)) {
        return;
    }
    let span = project
        .fact(module, event)
        .map_or_else(glass_lint_datastructures::ByteRange::empty, |fact| {
            fact.span
        });

    // Build the trace chain in execution order: source -> requirements -> sink.
    // Each node's parent is the previous step in execution order.
    // The last node (sink) is the trace head stored in the occurrence.
    // reconstruct_trace walks parent links backward and reverses.

    // Step 1: source event (first in execution order, no parent)
    let source_node = arena.intern(
        None,
        TraceQualifiedEvent::new(state.source.module, state.source.fact),
        EvidenceRole::Source,
    );

    // Step 2: requirement events (each has the previous step as parent)
    let mut tail = source_node;
    for req in state.requirements.values() {
        tail = match arena.intern(
            tail,
            TraceQualifiedEvent::new(req.module, req.fact),
            EvidenceRole::Requirement,
        ) {
            Some(id) => Some(id),
            None => break,
        };
    }

    // Step 3: sink event (last in execution order, becomes the trace head)
    let trace_head = tail.and_then(|prev| {
        arena.intern(
            Some(prev),
            TraceQualifiedEvent::new(module, event),
            EvidenceRole::Sink,
        )
    });

    values.evidence[rule_idx].push(ClassificationEvidence {
        kind: MatchKind::CallArgument,
        symbol: flow.evidence_symbol(),
        count: 1,
        truncated: false,
        certainty: crate::project::MatchCertainty::Definite,
        occurrences: vec![
            crate::api::classification::ClassificationEvidenceOccurrence {
                span,
                fact: Some(event.0),
                trace: trace_head,
            },
        ],
    });
}
