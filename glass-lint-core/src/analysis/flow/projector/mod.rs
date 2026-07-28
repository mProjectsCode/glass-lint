//! Bounded object flow over the canonical semantic fact stream.
//!
//! This projector owns no AST and performs no resolution.  `FactBuilder` has
//! already assigned value identities, effective call arguments, member chains,
//! and function targets.  The transfer state below only follows those typed
//! identities and emits evidence at canonical call sites.
//!
//! Control frames snapshot environments at joins, while transfer modules
//! update aliases and lifecycle requirements. Any unsupported or over-budget
//! path is discarded rather than converted into a speculative finding.

mod control;
mod evidence;
mod history;
mod state;
mod transfer;

use std::collections::BTreeMap;

use glass_lint_datastructures::{Budget, NameTable};
use state::{AbruptExit, ControlFrame, FlowEnvironment, FlowEvidence, FlowStateTable};

use crate::{
    analysis::{
        facts::{
            CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        },
        flow::{
            effect::{CallEffectRef, FunctionEffects},
            planning::BoundFlowPlan,
            summary::FunctionSummaries,
        },
        model::flow::{FlowLimits, FlowState},
        trace::TraceArena,
        value::{ObjectId, ValueId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind, RuleIndex},
        compiler::{CompiledObjectFlow, CompiledObjectRequirement},
    },
    project::ModuleId,
};

/// Exhaustion state and bounded counters returned by local flow projection.
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct LocalFlowProjectionOutcome {
    /// Whether any budget was exhausted during projection.
    pub exhausted: bool,
    /// Object identities allocated.
    #[allow(dead_code)]
    pub objects_used: u32,
}

/// Push flow evidence directly into an externally-owned per-rule vec,
/// avoiding a separate evidence matrix allocation alongside the caller's.
/// Returns the exhaustion state and bounded counters for the caller.
pub(in crate::analysis) fn collect_into(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[(RuleIndex, usize, &CompiledObjectFlow)],
    evidence: &mut [Vec<ClassificationEvidence>],
    limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> LocalFlowProjectionOutcome {
    let names = stream.names();
    let plan = BoundFlowPlan::new(rules, names);
    let mut summary_budget = Budget::new(limits.emission_limit());
    let helpers = FunctionSummaries::collect(stream, effects, &plan, &mut summary_budget);
    let mut projector = ObjectFlowProjector::new(
        stream,
        names,
        plan,
        helpers,
        evidence,
        limits,
        summary_budget.exhausted(),
        module_id,
        trace_arena,
    );
    for fact in stream.facts() {
        projector.transfer(fact);
    }
    projector.into_outcome()
}

#[cfg(test)]
pub(super) fn collect_with_limits(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[(RuleIndex, usize, &CompiledObjectFlow)],
    rule_count: usize,
    limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> (Vec<Vec<ClassificationEvidence>>, LocalFlowProjectionOutcome) {
    let mut evidence = vec![Vec::new(); rule_count];
    let outcome = collect_into(
        stream,
        effects,
        rules,
        &mut evidence,
        limits,
        module_id,
        trace_arena,
    );
    (evidence, outcome)
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules, 'stream, 'arena> {
    /// The canonical facts are the projector's only input. In particular, it
    /// must never inspect the AST or reconstruct resolution decisions.
    stream: &'stream FactStream<Frozen>,
    names: &'stream NameTable,
    plan: BoundFlowPlan<'rules>,
    helpers: FunctionSummaries<'stream>,
    /// Call results are indexed once so later assignments can start a flow
    /// without rescanning the fact stream.
    calls_by_result: BTreeMap<ValueId, FactId>,
    /// Evidence is grouped and deduplicated by the flow-specific evidence
    /// owner.
    flow_evidence: FlowEvidence<'stream>,
    /// Each value identity and live object-flow state are owned together.
    flow_state: FlowStateTable,
    /// Object IDs are local to one projection and bounded by `limits`.
    next_object_id: u32,
    /// Per-run hard limits for objects, states, and evidence emissions.
    limits: FlowLimits,
    /// Nested branch/function frames used to restore environments at joins.
    control: Vec<ControlFrame>,
    /// Facts after an unreachable branch are ignored until a joint restores a
    /// reachable environment.
    reachable: bool,
    /// Summary construction exhausted its budget.
    summary_exhausted: bool,
    /// Shared trace arena for interning evidence trace nodes.
    trace_arena: &'arena mut TraceArena,
    /// Module being projected, used to qualify trace events.
    module_id: ModuleId,
}

impl<'rules, 'stream, 'arena> ObjectFlowProjector<'rules, 'stream, 'arena> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        stream: &'stream FactStream<Frozen>,
        names: &'stream NameTable,
        plan: BoundFlowPlan<'rules>,
        helpers: FunctionSummaries<'stream>,
        evidence: &'stream mut [Vec<ClassificationEvidence>],
        limits: FlowLimits,
        summary_exhausted: bool,
        module_id: ModuleId,
        trace_arena: &'arena mut TraceArena,
    ) -> Self {
        let calls_by_result = stream
            .facts()
            .iter()
            .filter_map(|fact| match &fact.payload {
                FactPayload::Call { result, .. } => Some((*result, fact.id)),
                _ => None,
            })
            .collect();
        Self {
            stream,
            names,
            plan,
            helpers,
            calls_by_result,
            flow_evidence: FlowEvidence::new(evidence),
            flow_state: FlowStateTable::new(limits.state_limit(), limits.mutation_limit()),
            next_object_id: 0,
            limits,
            control: Vec::new(),
            reachable: true,
            summary_exhausted,
            module_id,
            trace_arena,
        }
    }

    fn transfer(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match &fact.payload {
            FactPayload::Function { boundary, .. } => self.transfer_function(*boundary),
            FactPayload::Control { kind, region, .. } => {
                self.transfer_control(*kind, *region);
            }
            FactPayload::Declaration { target, source } => {
                if !self.reachable {
                    return;
                }
                self.assign(*target, *source);
            }
            FactPayload::Assignment {
                target,
                source,
                receiver,
            } => {
                if !self.reachable {
                    return;
                }
                if let Some(receiver) = receiver {
                    self.invalidate_object(*receiver);
                } else {
                    self.assign(*target, *source);
                }
            }
            FactPayload::PropertyWrite {
                receiver,
                property,
                value,
            } => {
                if !self.reachable {
                    return;
                }
                let static_string = self.stream.values().static_string(*value);
                self.record_property_write(
                    *receiver,
                    property.and_then(|id| self.stream.resolve_name(id)),
                    static_string,
                    fact.id,
                );
            }
            FactPayload::Call { .. } => self.transfer_call(fact),
            _ => {}
        }
    }

    fn transfer_function(&mut self, boundary: FunctionBoundary) {
        match boundary {
            FunctionBoundary::Enter => {
                let caller = self.environment();
                self.control.push(ControlFrame::Function { caller });
                self.flow_state.clear();
                self.reachable = true;
            }
            FunctionBoundary::Exit => {
                if let Some(ControlFrame::Function { caller }) = self.control.pop() {
                    self.restore(caller);
                }
            }
        }
    }

    fn transfer_call(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        if !self.reachable {
            return;
        }
        let FactPayload::Call {
            receiver,
            target_function,
            args,
            ..
        } = &fact.payload
        else {
            return;
        };
        let cref = CallEffectRef {
            stream: self.stream,
            event: fact.id,
        };
        if let Some(chain) = cref.chain_owned(self.names) {
            let effective_args = cref.effective_args().unwrap_or(&[]);
            let rooted = cref.rooted();
            self.record_configuration(*receiver, &chain, effective_args, fact.id);
            self.record_sinks(&chain, effective_args, fact.id, rooted);
        }
        if let Some(function) = target_function {
            self.record_helper_sink(*function, args, fact.id);
        }
    }

    fn environment(&self) -> FlowEnvironment {
        self.flow_state.capture(self.reachable)
    }

    fn restore(&mut self, environment: FlowEnvironment) {
        self.reachable = self.flow_state.restore(environment);
    }

    fn join(&mut self, environments: &[FlowEnvironment]) {
        self.reachable = self.flow_state.join_environments(environments);
    }

    fn record_property_write(
        &mut self,
        receiver: ValueId,
        property: Option<&str>,
        value: Option<&str>,
        event: FactId,
    ) {
        let Some(object) = self.flow_state.object_for(receiver) else {
            return;
        };
        let keys = self
            .flow_state
            .states_for(object)
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        for key in keys {
            let Some(flow) = self.plan.get(key.flow) else {
                continue;
            };
            let Some(mut state) = self.flow_state.state_mut(key.object, key.flow) else {
                continue;
            };
            for (index, requirement) in flow.requirements.iter().enumerate() {
                if let CompiledObjectRequirement::PropertyWrite {
                    property: expected,
                    value: matcher,
                } = requirement
                    && (property.is_none() || property == Some(expected.as_str()))
                {
                    state.clear_requirement(index);
                    if property == Some(expected.as_str()) && matcher.matches_flow_value(value) {
                        state.record_requirement(index, event);
                    }
                }
            }
            drop(state);
            self.emit_if_ready(key.flow, key.object, event);
        }
    }

    fn unbind_value(&mut self, value: ValueId) {
        let Some(object) = self.flow_state.unbind(value) else {
            return;
        };
        if !self.flow_state.has_alias_for(object) {
            self.flow_state.remove_states_for(object);
        }
    }

    fn invalidate_object(&mut self, value: ValueId) {
        let Some(object) = self.flow_state.object_for(value) else {
            return;
        };
        self.flow_state.remove_states_for(object);
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object_id >= self.limits.object_limit() {
            return None;
        }
        let object = ObjectId(self.next_object_id);
        self.next_object_id = self.next_object_id.checked_add(1)?;
        Some(object)
    }

    /// Consume the projector and produce a bounded summary of what was used.
    fn into_outcome(self) -> LocalFlowProjectionOutcome {
        let exhausted = self.summary_exhausted
            || self.next_object_id >= self.limits.object_limit()
            || self.flow_state.state_count() >= self.limits.state_limit()
            || self.flow_evidence.emitted_count() >= self.limits.emission_limit()
            || self.flow_state.mutation_exhausted();
        LocalFlowProjectionOutcome {
            exhausted,
            objects_used: self.next_object_id,
        }
    }
}
#[cfg(test)]
mod tests;
