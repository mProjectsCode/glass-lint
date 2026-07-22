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
mod state;
mod transfer;

use std::collections::BTreeMap;

use state::{AbruptExit, ControlFrame, FlowEnvironment, FlowEvidence, FlowStateTable};

use crate::{
    analysis::{
        facts::{CallArgInfo, ControlKind, FactId, FactPayload, FactStream, FunctionBoundary},
        flow::{
            effect::{CallEffectRef, FunctionEffects},
            index::{FlowId, FlowIndex, FlowLimits},
            state::FlowState,
            summary::FunctionSummaries,
        },
        name::NameTable,
        value::{ObjectId, ValueId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind},
        compiler::{CompiledObjectFlow, CompiledObjectRequirement},
    },
};

pub(in crate::analysis) fn collect(
    stream: &FactStream,
    effects: &FunctionEffects,
    rules: &[(
        crate::api::classification::RuleIndex,
        usize,
        &CompiledObjectFlow,
    )],
    rule_count: usize,
) -> Vec<Vec<ClassificationEvidence>> {
    collect_with_limits(stream, effects, rules, rule_count, FlowLimits::default())
}

pub(super) fn collect_with_limits(
    stream: &FactStream,
    effects: &FunctionEffects,
    rules: &[(
        crate::api::classification::RuleIndex,
        usize,
        &CompiledObjectFlow,
    )],
    rule_count: usize,
    limits: FlowLimits,
) -> Vec<Vec<ClassificationEvidence>> {
    // Helpers are summarized before projection so a selected flow rule never
    // changes the canonical fact walk or requires another AST traversal.
    let Some(names) = stream.names() else {
        return vec![Vec::new(); rule_count];
    };
    let flow_index = FlowIndex::new(rules, names);
    let helpers = FunctionSummaries::collect(stream, effects, &flow_index);
    let mut projector =
        ObjectFlowProjector::new(stream, names, flow_index, helpers, rule_count, limits);
    for fact in stream.facts() {
        projector.transfer(fact);
    }
    projector.flow_evidence.into_items()
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules, 'stream> {
    /// The canonical facts are the projector's only input. In particular, it
    /// must never inspect the AST or reconstruct resolution decisions.
    stream: &'stream FactStream,
    names: &'stream NameTable,
    flow_index: FlowIndex<'rules>,
    helpers: FunctionSummaries<'stream>,
    /// Call results are indexed once so later assignments can start a flow
    /// without rescanning the fact stream.
    calls_by_result: BTreeMap<ValueId, FactId>,
    /// Evidence is grouped and deduplicated by the flow-specific evidence
    /// owner.
    flow_evidence: FlowEvidence,
    /// Each value identity and live object-flow state are owned together.
    flow_state: FlowStateTable,
    /// Object IDs are local to one projection and bounded by `limits`.
    next_object_id: u32,
    /// Per-run hard limits for objects, states, and evidence emissions.
    limits: FlowLimits,
    /// Nested branch/function frames used to restore environments at joins.
    control: Vec<ControlFrame>,
    /// Facts after an unreachable branch are ignored until a join restores a
    /// reachable environment.
    reachable: bool,
}

impl<'rules, 'stream> ObjectFlowProjector<'rules, 'stream> {
    fn new(
        stream: &'stream FactStream,
        names: &'stream NameTable,
        flow_index: FlowIndex<'rules>,
        helpers: FunctionSummaries<'stream>,
        rule_count: usize,
        limits: FlowLimits,
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
            flow_index,
            helpers,
            calls_by_result,
            flow_evidence: FlowEvidence::new(rule_count),
            flow_state: FlowStateTable::default(),
            next_object_id: 0,
            limits,
            control: Vec::new(),
            reachable: true,
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
                let static_string = self
                    .stream
                    .values()
                    .and_then(|values| values.static_string(*value));
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
            let Some(flow) = self.flow_index.get(key.flow) else {
                continue;
            };
            let Some(state) = self.flow_state.state_mut(key.object, key.flow) else {
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
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis::resolution::Resolver,
        api::rule::{
            FlowCompletion, FlowCondition, FlowSinkMatcher, MemberCallMatcher, ObjectEventMatcher,
            ObjectFlowMatcher, ObjectSourceMatcher, ValueMatcher,
        },
    };

    fn collect_source(source: &str, flow: &ObjectFlowMatcher) -> Vec<Vec<ClassificationEvidence>> {
        let parsed = crate::parse(source, "fact-flow.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let flow = CompiledObjectFlow::from_matcher(flow);
        collect_with_limits(
            &stream,
            &effects,
            &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
            1,
            FlowLimits::default(),
        )
    }

    fn script_flow() -> ObjectFlowMatcher {
        ObjectFlowMatcher::builder("script insertion")
            .source(ObjectSourceMatcher::returned_by(
                MemberCallMatcher::rooted("document.createElement")
                    .arg(0, ValueMatcher::static_string().equals("script")),
            ))
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "src",
                ValueMatcher::any_value(),
            )))
            .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                MemberCallMatcher::rooted("document.head.appendChild"),
                0,
            )]))
            .build()
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
        let flow = ObjectFlowMatcher::builder("configured script")
            .source(ObjectSourceMatcher::returned_by(
                MemberCallMatcher::rooted("document.createElement")
                    .arg(0, ValueMatcher::static_string().equals("script")),
            ))
            .configured_by(FlowCondition::event(
                ObjectEventMatcher::member_call("configure")
                    .arg(0, ValueMatcher::static_string().equals("yes")),
            ))
            .complete_at(FlowCompletion::any_sink([FlowSinkMatcher::argument_of(
                MemberCallMatcher::rooted("document.head.appendChild"),
                0,
            )]))
            .build();
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
        let mut resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let sink_span = stream
            .facts()
            .iter()
            .find_map(|fact| match &fact.payload {
                FactPayload::Call {
                    syntactic_path: Some(chain),
                    ..
                } if chain
                    .to_symbol_path(stream.names().unwrap())
                    .is_some_and(|s| s.eq_chain("document.head.appendChild")) =>
                {
                    Some(fact.span)
                }
                _ => None,
            })
            .expect("sink call should be present");
        let flow = CompiledObjectFlow::from_matcher(&script_flow());
        let evidence = collect_with_limits(
            &stream,
            &effects,
            &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
            1,
            FlowLimits::default(),
        );
        assert_eq!(evidence[0][0].occurrences[0].span, sink_span);
    }

    #[test]
    fn requirement_only_evidence_is_anchored_at_the_configuration_event() {
        let flow = ObjectFlowMatcher::builder("configured input")
            .source(ObjectSourceMatcher::returned_by(
                MemberCallMatcher::rooted("document.createElement")
                    .arg(0, ValueMatcher::static_string().equals("input")),
            ))
            .configured_by(FlowCondition::event(ObjectEventMatcher::property_write(
                "type",
                ValueMatcher::static_string().equals("file"),
            )))
            .complete_at(FlowCompletion::configuration())
            .build();
        let source = "const input = document.createElement('input'); input.type = 'file';";
        let parsed =
            crate::parse(source, "flow-requirement-location.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::analysis::facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        let configuration = stream
            .facts()
            .iter()
            .find_map(|fact| {
                matches!(fact.payload, FactPayload::PropertyWrite { .. })
                    .then_some((fact.id, fact.span))
            })
            .expect("configuration write should be present");
        let flow = CompiledObjectFlow::from_matcher(&flow);
        let evidence = collect_with_limits(
            &stream,
            &effects,
            &[(crate::api::classification::RuleIndex::new(0), 0, &flow)],
            1,
            FlowLimits::default(),
        );
        assert_eq!(evidence[0][0].occurrences[0].span, configuration.1);
        assert_eq!(evidence[0][0].occurrences[0].fact, Some(configuration.0.0));
    }
}
