use std::collections::{BTreeMap, BTreeSet};

use smol_str::SmolStr;

use super::{
    CallContext, ContextWorklist, CrossFlowState, QualifiedEvent, chain_matches, effect_use_event,
    emit, usage_matches_context,
};
use crate::{
    analysis::{
        ProjectSemanticModel,
        facts::FactId,
        flow::effect::{EffectUse, FunctionEffect},
        name::NameTable,
    },
    api::{classification::ClassificationEvidence, compiler::CompiledObjectFlow},
    project::ModuleId,
};

pub(super) struct UsageProjector<'a> {
    pub(super) project: &'a ProjectSemanticModel,
    pub(super) evidence: &'a mut BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>,
    pub(super) context: &'a CallContext,
    pub(super) effect: &'a FunctionEffect,
    pub(super) flow: &'a CompiledObjectFlow,
    pub(super) state: &'a mut CrossFlowState,
    pub(super) propagated: &'a mut BTreeSet<FactId>,
    pub(super) worklist: &'a mut ContextWorklist,
    pub(super) names: &'a NameTable,
}

impl UsageProjector<'_> {
    pub(super) fn project(&mut self) {
        for usage in self.effect.uses() {
            if !usage_matches_context(self.effect, usage, self.context) {
                continue;
            }
            CallPropagation {
                project: self.project,
                effect: self.effect,
                module: self.context.module,
                context: self.context,
                propagated: self.propagated,
                through: Some(effect_use_event(usage)),
                state: self.state,
                worklist: self.worklist,
            }
            .propagate();
            match usage {
                EffectUse::PropertyWrite {
                    event, property, ..
                } => self.apply_property(*event, property.as_ref()),
                EffectUse::CallReceiver { event, chain, .. } => {
                    self.apply_receiver(*event, chain.as_ref());
                }
                EffectUse::CallArgument {
                    event,
                    chain,
                    rooted,
                    argument,
                } => self.apply_argument(*event, chain.as_ref(), *rooted, argument.index()),
            }
        }
    }

    fn apply_property(&mut self, event: FactId, property: Option<&SmolStr>) {
        let static_value = self
            .project
            .module_fact_stream(self.context.module)
            .and_then(|stream| {
                let value = stream.property_write_value(event)?;
                stream.values()?.static_string(value)
            });
        let mut next = self.state.clone();
        for (index, requirement) in self.flow.requirements.iter().enumerate() {
            if let crate::api::compiler::CompiledObjectRequirement::PropertyWrite {
                property: expected,
                value,
            } = requirement
                && property == Some(expected)
                && value.matches_flow_value(static_value)
            {
                next.requirements.insert(
                    index,
                    QualifiedEvent {
                        module: self.context.module,
                        fact: event,
                    },
                );
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_receiver(&mut self, event: FactId, chain: Option<&crate::analysis::value::NamePath>) {
        let Some(call_args) = self
            .project
            .module_names(self.context.module)
            .and_then(|_| {
                let stream = self.project.module_fact_stream(self.context.module)?;
                stream.call_args_for_event(event)
            })
        else {
            return;
        };
        let values = self
            .project
            .module_fact_stream(self.context.module)
            .and_then(|s| s.values());
        let mut next = self.state.clone();
        for (index, requirement) in self.flow.requirements.iter().enumerate() {
            if let crate::api::compiler::CompiledObjectRequirement::MemberCall { member, arguments } =
                requirement
                && chain_matches(chain, member, self.names)
                && arguments.iter().all(|matcher| {
                    call_args
                        .get(matcher.index())
                        .is_some_and(|argument| match values {
                            Some(values) => matcher.matcher().matches(argument, self.names, values),
                            None => false,
                        })
                })
            {
                next.requirements.insert(
                    index,
                    QualifiedEvent {
                        module: self.context.module,
                        fact: event,
                    },
                );
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_argument(
        &mut self,
        event: FactId,
        chain: Option<&crate::analysis::value::NamePath>,
        rooted: bool,
        argument: usize,
    ) {
        if self.flow.sink_matches(
            chain
                .and_then(|chain| chain.to_symbol_path(self.names))
                .as_ref(),
            rooted,
            argument,
        ) && self.flow.requirements_ready(self.state.requirements.len())
            && self.context.crossed
        {
            emit(
                self.project,
                self.evidence,
                self.context.module,
                self.context.state.flow,
                self.state,
                event,
                self.flow,
            );
        }
    }

    fn emit_requirements(&mut self, state: &CrossFlowState, event: FactId) {
        if self.flow.emit_on_requirements
            && self.flow.requirements_ready(state.requirements.len())
            && self.context.crossed
        {
            emit(
                self.project,
                self.evidence,
                self.context.module,
                self.context.state.flow,
                state,
                event,
                self.flow,
            );
        }
    }
}

pub(super) struct CallPropagation<'a> {
    pub(super) project: &'a ProjectSemanticModel,
    pub(super) effect: &'a FunctionEffect,
    pub(super) module: ModuleId,
    pub(super) context: &'a CallContext,
    pub(super) propagated: &'a mut BTreeSet<FactId>,
    pub(super) through: Option<FactId>,
    pub(super) state: &'a CrossFlowState,
    pub(super) worklist: &'a mut ContextWorklist,
}

impl CallPropagation<'_> {
    pub(super) fn propagate(&mut self) {
        for call in self.effect.calls() {
            if self.through.is_some_and(|event| call.event() > event)
                || !self.propagated.insert(call.event())
            {
                continue;
            }
            let Some((target_module, target_function)) = self.project.qualified_function_target(
                self.module,
                call.target(),
                call.provenance(),
            ) else {
                continue;
            };
            for argument in call.arguments() {
                let connected = argument.parameter().is_some_and(|parameter| {
                    self.context
                        .parameter
                        .is_some_and(|index| parameter.index() == index)
                        && parameter.is_root()
                        && argument.is_root()
                }) || (self.context.parameter.is_none()
                    && self.context.source_root.is_some_and(|root| {
                        self.effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value())
                            == root
                    })
                    && argument.is_root());
                if connected {
                    self.worklist.enqueue_parameters(
                        self.project,
                        target_module,
                        target_function,
                        argument.index(),
                        self.state,
                        self.context.crossed || target_module != self.module,
                    );
                }
            }
        }
    }
}
