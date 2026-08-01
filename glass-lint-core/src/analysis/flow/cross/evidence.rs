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
    /// Sink occurrences reached by an alternative that did not produce a
    /// complete witness. These keys are kept separate from witness traces so
    /// no evidence can be assembled from incompatible call sites.
    pub(super) nonmatching: Vec<BTreeSet<(MatchKind, String, u32)>>,
    pub(super) trace_heads: usize,
}

impl ModuleEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            evidence: vec![Vec::new(); rule_count],
            seen: vec![BTreeSet::new(); rule_count],
            nonmatching: vec![BTreeSet::new(); rule_count],
            trace_heads: 0,
        }
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
    let rule_idx = flow_id.rule_index().get();
    let key = (
        MatchKind::CallArgument,
        flow.symbol.as_str().to_owned(),
        event.0,
    );
    values.nonmatching[rule_idx].insert(key.clone());
    if let Some(item) = values.evidence[rule_idx].iter_mut().find(|item| {
        item.kind == key.0
            && item.symbol == key.1
            && item
                .occurrences
                .iter()
                .any(|occurrence| occurrence.fact == Some(key.2))
    }) {
        item.certainty = crate::project::MatchCertainty::Possible;
    }
}

pub(super) struct EmissionContext<'a> {
    pub(super) project: &'a ProjectSemanticModel,
    pub(super) evidence: &'a mut HashMap<ModuleId, ModuleEvidence>,
    pub(super) arena: &'a mut TraceArena,
}

#[allow(clippy::too_many_lines)]
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
    let rule_idx = flow_id.rule_index().get();
    let key = (
        MatchKind::CallArgument,
        flow.symbol.as_str().to_owned(),
        event.0,
    );
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
    let Some(source) = state.source.as_ref() else {
        return;
    };
    let source_node = arena.intern(
        None,
        TraceQualifiedEvent::new(source.module, source.fact),
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

    // Preserve earlier sink events for all-sink correlations. Fact order is
    // the deterministic execution order; the completing sink is emitted once
    // below as the terminal sink node.
    let mut prior_sinks: Vec<_> = state
        .sinks
        .values()
        .filter(|sink| !(sink.module == module && sink.fact == event))
        .cloned()
        .collect();
    prior_sinks.sort();
    prior_sinks.dedup();
    for sink in prior_sinks {
        tail = match arena.intern(
            tail,
            TraceQualifiedEvent::new(sink.module, sink.fact),
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

    let certainty = if values.nonmatching[rule_idx].contains(&key) {
        crate::project::MatchCertainty::Possible
    } else {
        crate::project::MatchCertainty::Definite
    };
    let occurrence = crate::api::classification::ClassificationEvidenceOccurrence {
        span,
        fact: Some(event.0),
        trace: trace_head,
    };
    if let Some(item) = values.evidence[rule_idx].iter_mut().find(|item| {
        item.kind == key.0
            && item.symbol == key.1
            && item
                .occurrences
                .iter()
                .any(|existing| existing.fact == Some(event.0) && existing.span == span)
    }) {
        item.certainty = if item.certainty == crate::project::MatchCertainty::Possible
            || certainty == crate::project::MatchCertainty::Possible
        {
            crate::project::MatchCertainty::Possible
        } else {
            crate::project::MatchCertainty::Definite
        };
        if !item
            .occurrences
            .iter()
            .any(|existing| existing.trace == occurrence.trace)
        {
            item.occurrences.push(occurrence);
            item.count = item.count.saturating_add(1);
        }
    } else {
        values.seen[rule_idx].insert(key);
        values.evidence[rule_idx].push(ClassificationEvidence {
            kind: MatchKind::CallArgument,
            symbol: flow.evidence_symbol().as_str().to_owned(),
            count: 1,
            truncated: false,
            certainty,
            occurrences: vec![occurrence],
        });
    }
    if trace_head.is_some() {
        values.trace_heads = values.trace_heads.saturating_add(1);
    }
}
