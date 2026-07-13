//! Bounded object flow over the canonical semantic fact stream.
//!
//! This projector owns no AST and performs no resolution.  `FactBuilder` has
//! already assigned value identities, effective call arguments, member chains,
//! and function targets.  The transfer state below only follows those typed
//! identities and emits evidence at canonical call sites.

mod control;
mod evidence;
mod transfer;

use transfer::SourceCall;

use std::collections::{BTreeMap, BTreeSet};

use super::super::facts::{
    CallArgInfo, ControlKind, FactId, FactPayload, FactStream, FunctionBoundary,
};
use super::super::value::{ObjectId, ValueId};
use super::index::{FlowId, FlowIndex, FlowLimits};
use super::state::FlowState;
use super::summary::FunctionSummaries;
use crate::api::classification::{ApiEvidence, ApiMatchKind};
use crate::api::compiler::{CompiledObjectFlow, CompiledObjectRequirement};

pub(in crate::analysis) fn collect(
    stream: &FactStream,
    rules: &[(usize, usize, &CompiledObjectFlow)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    collect_with_limits(stream, rules, rule_count, FlowLimits::default())
}

pub(super) fn collect_with_limits(
    stream: &FactStream,
    rules: &[(usize, usize, &CompiledObjectFlow)],
    rule_count: usize,
    limits: FlowLimits,
) -> Vec<Vec<ApiEvidence>> {
    let flow_index = FlowIndex::new(rules);
    let helpers = super::summary::collect(stream, &flow_index);
    let mut projector = ObjectFlowProjector::new(stream, flow_index, helpers, rule_count, limits);
    for fact in stream.facts() {
        projector.transfer(fact);
    }
    projector.evidence
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules, 'stream> {
    /// The canonical facts are the projector's only input. In particular, it
    /// must never inspect the AST or reconstruct resolution decisions.
    stream: &'stream FactStream,
    flow_index: FlowIndex<'rules>,
    helpers: FunctionSummaries,
    /// Call results are indexed once so later assignments can start a flow
    /// without rescanning the fact stream.
    calls_by_result: BTreeMap<ValueId, SourceCall>,
    /// Evidence uses the exact fact that established a match as its anchor.
    fact_spans: BTreeMap<FactId, swc_common::Span>,
    /// Evidence is grouped by rule index to preserve catalog ordering.
    evidence: Vec<Vec<ApiEvidence>>,
    /// Each value identity points to at most one object-flow identity.
    aliases: BTreeMap<ValueId, ObjectId>,
    /// Live flow state is keyed by object and compiled flow, not by syntax.
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    /// Prevent duplicate evidence for the same object/flow/event combination.
    emitted: BTreeSet<(usize, usize, ObjectId, FactId)>,
    /// Object IDs are local to one projection and bounded by `limits`.
    next_object_id: u32,
    limits: FlowLimits,
    control: Vec<ControlFrame>,
    /// Facts after an unreachable branch are ignored until a join restores a
    /// reachable environment.
    reachable: bool,
}

/// The portion of projector state that can cross a control-flow boundary.
///
/// Keeping this separate from `ObjectFlowProjector` makes branch joins
/// explicit: a join combines only facts that are true on every reachable
/// incoming path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowEnvironment {
    aliases: BTreeMap<ValueId, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    reachable: bool,
}

#[derive(Debug, Clone)]
enum ControlFrame {
    Branch {
        region: u32,
        base: FlowEnvironment,
        then_exit: Option<FlowEnvironment>,
    },
    Loop {
        region: u32,
        baseline: FlowEnvironment,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: u32,
        baseline: FlowEnvironment,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: u32,
        baseline: FlowEnvironment,
        try_exit: Option<FlowEnvironment>,
        catch_exit: Option<FlowEnvironment>,
        normal_exit: Option<FlowEnvironment>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
    },
    Function {
        caller: FlowEnvironment,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbruptExit {
    Break,
    Continue,
    Return,
}

impl FlowEnvironment {
    fn unreachable() -> Self {
        Self {
            aliases: BTreeMap::new(),
            states: BTreeMap::new(),
            reachable: false,
        }
    }

    /// Intersect two path states. An alias or requirement is retained only
    /// when both paths agree, which prevents one-arm configuration from
    /// leaking through a join.
    fn join(left: &Self, right: &Self) -> Self {
        if !left.reachable {
            return right.clone();
        }
        if !right.reachable {
            return left.clone();
        }
        let aliases = left
            .aliases
            .iter()
            .filter_map(|(binding, object)| {
                (right.aliases.get(binding) == Some(object)).then_some((*binding, *object))
            })
            .collect();
        let states = left
            .states
            .iter()
            .filter_map(|(key, left_state)| {
                let right_state = right.states.get(key)?;
                let mut state = left_state.clone();
                state
                    .requirements
                    .retain(|requirement, _| right_state.requirements.contains_key(requirement));
                Some((*key, state))
            })
            .collect();
        Self {
            aliases,
            states,
            reachable: true,
        }
    }

    /// Join all candidate exits, ignoring paths that are already unreachable.
    fn join_many(environments: &[Self]) -> Self {
        let Some(first) = environments
            .iter()
            .find(|environment| environment.reachable)
        else {
            return Self::unreachable();
        };
        environments
            .iter()
            .filter(|environment| environment.reachable)
            .skip(1)
            .fold(first.clone(), |joined, environment| {
                Self::join(&joined, environment)
            })
    }

    fn capture(projector: &ObjectFlowProjector<'_, '_>) -> Self {
        Self {
            aliases: projector.aliases.clone(),
            states: projector.states.clone(),
            reachable: projector.reachable,
        }
    }
}

impl<'rules, 'stream> ObjectFlowProjector<'rules, 'stream> {
    fn new(
        stream: &'stream FactStream,
        flow_index: FlowIndex<'rules>,
        helpers: FunctionSummaries,
        rule_count: usize,
        limits: FlowLimits,
    ) -> Self {
        let calls_by_result = stream
            .facts()
            .iter()
            .filter_map(|fact| match &fact.payload {
                FactPayload::Call { result, .. } => Some((*result, SourceCall::from_fact(fact)?)),
                _ => None,
            })
            .collect();
        let fact_spans = stream
            .facts()
            .iter()
            .map(|fact| (fact.id, fact.span))
            .collect();

        Self {
            stream,
            flow_index,
            helpers,
            calls_by_result,
            fact_spans,
            evidence: vec![Vec::new(); rule_count],
            aliases: BTreeMap::new(),
            states: BTreeMap::new(),
            emitted: BTreeSet::new(),
            next_object_id: 0,
            limits,
            control: Vec::new(),
            reachable: true,
        }
    }

    fn transfer(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match &fact.payload {
            FactPayload::Function { boundary, .. } => match boundary {
                FunctionBoundary::Enter => {
                    let caller = self.environment();
                    self.control.push(ControlFrame::Function { caller });
                    self.aliases.clear();
                    self.states.clear();
                    self.reachable = true;
                }
                FunctionBoundary::Exit => {
                    if let Some(ControlFrame::Function { caller }) = self.control.pop() {
                        self.restore(caller);
                    }
                }
            },
            FactPayload::Control { kind, region } => {
                self.transfer_control(*kind, *region, fact.span);
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
                static_value,
                ..
            } => {
                if !self.reachable {
                    return;
                }
                self.record_property_write(
                    *receiver,
                    property.as_deref(),
                    static_value.as_deref(),
                    fact.id,
                );
            }
            FactPayload::Call {
                syntactic_chain,
                rooted_chain,
                callee_name,
                receiver,
                args,
                unwrap,
                target_function,
                ..
            } => {
                if !self.reachable {
                    return;
                }
                if let Some(source) = SourceCall::from_parts(
                    fact.id,
                    rooted_chain.as_deref(),
                    syntactic_chain.as_deref(),
                    callee_name.as_deref(),
                    args,
                    unwrap.as_deref(),
                ) {
                    self.record_configuration(
                        *receiver,
                        &source.chain,
                        &source.args,
                        source.fact_id,
                    );
                    self.record_sinks(&source.chain, &source.args, source.fact_id, source.rooted);
                }
                if let Some(function) = target_function {
                    self.record_helper_sink(*function, args, fact.id);
                }
            }
            _ => {}
        }
    }

    fn environment(&self) -> FlowEnvironment {
        FlowEnvironment::capture(self)
    }

    fn restore(&mut self, environment: FlowEnvironment) {
        self.aliases = environment.aliases;
        self.states = environment.states;
        self.reachable = environment.reachable;
    }

    fn record_property_write(
        &mut self,
        receiver: ValueId,
        property: Option<&str>,
        value: Option<&str>,
        event: FactId,
    ) {
        let Some(object) = self.aliases.get(&receiver).copied() else {
            return;
        };
        let keys = self
            .states
            .keys()
            .filter(|(id, _)| *id == object)
            .copied()
            .collect::<Vec<_>>();
        for key in keys {
            let Some(flow) = self.flow_index.get(key.1) else {
                continue;
            };
            let Some(state) = self.states.get_mut(&key) else {
                continue;
            };
            for (index, requirement) in flow.requirements.iter().enumerate() {
                if let CompiledObjectRequirement::PropertyWrite {
                    property: expected,
                    value: matcher,
                } = requirement
                    && (property.is_none() || property == Some(expected.as_str()))
                {
                    state.requirements.remove(&index);
                    if property == Some(expected.as_str()) && matcher.matches_flow_value(value) {
                        state.requirements.insert(index, event);
                    }
                }
            }
            self.emit_if_ready(key.1, key.0, event);
        }
    }

    fn unbind_value(&mut self, value: ValueId) {
        let Some(object) = self.aliases.remove(&value) else {
            return;
        };
        if !self.aliases.values().any(|alias| *alias == object) {
            self.states.retain(|(id, _), _| *id != object);
        }
    }

    fn invalidate_object(&mut self, value: ValueId) {
        let Some(object) = self.aliases.get(&value).copied() else {
            return;
        };
        self.states.retain(|(id, _), _| *id != object);
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object_id >= self.limits.objects {
            return None;
        }
        let object = ObjectId(self.next_object_id);
        self.next_object_id = self.next_object_id.checked_add(1)?;
        Some(object)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::resolution::Resolver;
    use crate::api::rule::{FlowMatcher, FlowValueMatcher};

    fn collect_source(source: &str, flow: &FlowMatcher) -> Vec<Vec<ApiEvidence>> {
        let parsed = crate::parse(source, "fact-flow.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = crate::analysis::facts::build::build_test_stream(&parsed.program, &resolver);
        let flow = crate::api::compiler::compile_legacy_flow(flow.clone());
        collect_with_limits(&stream, &[(0, 0, &flow)], 1, FlowLimits::default())
    }

    fn script_flow() -> FlowMatcher {
        FlowMatcher::new("script insertion")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .property_write("src", FlowValueMatcher::Any)
            .sink_member_call_arg_indices(["document.head.appendChild"], [0])
    }

    #[test]
    fn transfers_source_configuration_and_sink_from_facts() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn member_call_configuration_stays_with_its_receiver() {
        let flow = FlowMatcher::new("configured script")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .member_call_config(
                "configure",
                [(0, FlowValueMatcher::StaticExact(vec!["yes".into()]))],
            )
            .sink_member_call_arg_indices(["document.head.appendChild"], [0]);
        let evidence = collect_source(
            "const first = document.createElement('script'); const second = document.createElement('script'); first.configure('yes'); document.head.appendChild(second); document.head.appendChild(first);",
            &flow,
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn property_invalidation_is_driven_by_assignment_facts() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; script.src += suffix; document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn separate_sink_facts_produce_separate_match_occurrences() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 2);
    }

    #[test]
    fn unchanged_branch_paths_retain_baseline_state() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; if (ready) {} document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn identical_branch_requirements_are_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); if (ready) { script.src = url; } else { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn one_arm_requirement_does_not_leak_after_join() {
        let evidence = collect_source(
            "const script = document.createElement('script'); if (ready) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn zero_iteration_loops_do_not_make_body_configuration_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); while (ready) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn do_while_body_configuration_is_reachable_after_loop() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { script.src = url; } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn catch_only_configuration_does_not_become_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); try { work(); } catch (error) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn catch_sink_can_consume_a_source_from_before_try() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; try { work(); } catch (error) { document.head.appendChild(script); }",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_configuration_is_applied_to_normal_completion() {
        let evidence = collect_source(
            "const script = document.createElement('script'); try { work(); } finally { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn switch_no_match_path_prevents_case_only_configuration() {
        let evidence = collect_source(
            "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn default_case_can_make_configuration_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; default: script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn do_while_break_preserves_the_break_exit() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { script.src = url; break; } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_configuration_reaches_a_break_exit() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { try { break; } finally { script.src = url; } } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_return_does_not_reach_code_after_the_try() {
        let evidence = collect_source(
            "function run() { const script = document.createElement('script'); try { return; } finally { script.src = url; } document.head.appendChild(script); }",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn destructuring_assignment_invalidates_the_written_alias() {
        let evidence = collect_source(
            "let script = document.createElement('script'); script.src = url; ({ script } = replacement); document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn rebinding_one_alias_does_not_kill_the_shared_object() {
        let evidence = collect_source(
            "let first = document.createElement('script'); const alias = first; first = replacement; alias.src = url; document.head.appendChild(alias);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn flow_evidence_is_anchored_at_the_sink_event() {
        let source = "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);";
        let parsed = crate::parse(source, "flow-location.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = crate::analysis::facts::build::build_test_stream(&parsed.program, &resolver);
        let sink_span = stream
            .facts()
            .iter()
            .find_map(|fact| match &fact.payload {
                FactPayload::Call {
                    syntactic_chain: Some(chain),
                    ..
                } if chain == "document.head.appendChild" => Some(fact.span),
                _ => None,
            })
            .expect("sink call should be present");
        let flow = crate::api::compiler::compile_legacy_flow(script_flow());
        let evidence = collect_with_limits(&stream, &[(0, 0, &flow)], 1, FlowLimits::default());
        assert_eq!(evidence[0][0].spans, vec![sink_span]);
    }

    #[test]
    fn requirement_only_evidence_is_anchored_at_the_configuration_event() {
        let flow = FlowMatcher::new("configured input")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["input"])
            .property_write("type", FlowValueMatcher::StaticExact(vec!["file".into()]))
            .emit_when_requirements_met();
        let source = "const input = document.createElement('input'); input.type = 'file';";
        let parsed =
            crate::parse(source, "flow-requirement-location.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream = crate::analysis::facts::build::build_test_stream(&parsed.program, &resolver);
        let configuration = stream
            .facts()
            .iter()
            .find_map(|fact| {
                matches!(fact.payload, FactPayload::PropertyWrite { .. })
                    .then_some((fact.id, fact.span))
            })
            .expect("configuration write should be present");
        let flow = crate::api::compiler::compile_legacy_flow(flow);
        let evidence = collect_with_limits(&stream, &[(0, 0, &flow)], 1, FlowLimits::default());
        assert_eq!(evidence[0][0].spans, vec![configuration.1]);
        assert_eq!(evidence[0][0].event_ids, vec![configuration.0.0]);
    }
}
