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

    fn mark_nonmatching(&mut self, rule_index: usize, key: &EvidenceKey) {
        let rule = &mut self.rules[rule_index];
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

    fn record(&mut self, rule_index: usize, key: &EvidenceKey, mut item: ClassificationEvidence) {
        let rule = &mut self.rules[rule_index];
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

    pub(super) fn clear(&mut self) {
        for rule in &mut self.rules {
            rule.items.clear();
        }
    }

    pub(super) fn into_evidence(self) -> Vec<Vec<ClassificationEvidence>> {
        self.rules.into_iter().map(|rule| rule.items).collect()
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

    let certainty = if values.rules[rule_idx].nonmatching.contains(&key) {
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
