//! Matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.

use std::collections::BTreeMap;

use super::super::facts::{CallArgInfo, ControlKind, FactPayload, FactStream, ParameterBinding};
use super::super::syntax::SymbolCallProvenance;
use super::super::value::{FunctionId, PathId, ValueId};
use super::table::FunctionTable;
use crate::budget::BudgetTracker;

const MAX_FUNCTION_EFFECTS: usize = 65_536;
const MAX_EFFECT_CALLS: usize = 65_536;
const MAX_EFFECT_USES: usize = 131_072;
const MAX_EFFECT_RETURNS: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct ParameterRef {
    index: usize,
    path: PathId,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectArgument {
    index: usize,
    value: ValueId,
    path: PathId,
    parameter: Option<ParameterRef>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectCall {
    event: super::super::facts::FactId,
    chain: Option<String>,
    rooted: bool,
    target: Option<FunctionId>,
    result: ValueId,
    provenance: SymbolCallProvenance,
    arguments: Vec<EffectArgument>,
    call_arguments: Vec<CallArgInfo>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) enum EffectUse {
    PropertyWrite {
        event: super::super::facts::FactId,
        receiver: Option<ParameterRef>,
        value: ValueId,
        property: Option<String>,
        static_value: Option<String>,
    },
    CallArgument {
        event: super::super::facts::FactId,
        chain: Option<String>,
        rooted: bool,
        argument: EffectArgument,
    },
    CallReceiver {
        event: super::super::facts::FactId,
        chain: Option<String>,
        receiver: ParameterRef,
        call_arguments: Vec<CallArgInfo>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct FunctionEffect {
    id: FunctionId,
    parameters: Vec<ParameterBinding>,
    calls: Vec<EffectCall>,
    uses: Vec<EffectUse>,
    returns: Vec<ReturnProjection>,
    invalid: bool,
    /// Source-order value copies. Project flow uses this to connect a source
    /// call result through local declarations before a qualified call.
    value_roots: BTreeMap<ValueId, ValueId>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct ReturnProjection {
    value: ValueId,
    parameter: Option<ParameterRef>,
    provenance: SymbolCallProvenance,
    static_string: Option<String>,
}

impl ParameterRef {
    pub(in crate::analysis) fn index(&self) -> usize {
        self.index
    }
    pub(in crate::analysis) fn is_root(&self) -> bool {
        self.path.is_empty()
    }
}

impl EffectArgument {
    pub(in crate::analysis) fn index(&self) -> usize {
        self.index
    }
    pub(in crate::analysis) fn value(&self) -> ValueId {
        self.value
    }
    pub(in crate::analysis) fn parameter(&self) -> Option<&ParameterRef> {
        self.parameter.as_ref()
    }
    pub(in crate::analysis) fn is_root(&self) -> bool {
        self.path.is_empty()
    }
}

impl EffectCall {
    pub(in crate::analysis) fn event(&self) -> super::super::facts::FactId {
        self.event
    }
    pub(in crate::analysis) fn chain(&self) -> Option<&str> {
        self.chain.as_deref()
    }
    pub(in crate::analysis) fn is_rooted(&self) -> bool {
        self.rooted
    }
    pub(in crate::analysis) fn target(&self) -> Option<FunctionId> {
        self.target
    }
    pub(in crate::analysis) fn result(&self) -> ValueId {
        self.result
    }
    pub(in crate::analysis) fn provenance(&self) -> &SymbolCallProvenance {
        &self.provenance
    }
    pub(in crate::analysis) fn arguments(&self) -> &[EffectArgument] {
        &self.arguments
    }
    pub(in crate::analysis) fn call_arguments(&self) -> &[CallArgInfo] {
        &self.call_arguments
    }

    pub(in crate::analysis) fn matches_source(
        &self,
        flow: &crate::api::compiler::CompiledObjectFlow,
    ) -> bool {
        flow.sources.iter().any(|source| {
            self.chain() == Some(source.member_call.as_str())
                && source.provenance.matches_rooted(self.is_rooted())
                && source.arguments.iter().all(|matcher| {
                    self.call_arguments()
                        .get(matcher.index)
                        .is_some_and(|argument| matcher.matcher.matches(argument))
                })
        })
    }
}

impl ReturnProjection {
    pub(in crate::analysis) fn value(&self) -> ValueId {
        self.value
    }
    pub(in crate::analysis) fn parameter(&self) -> Option<&ParameterRef> {
        self.parameter.as_ref()
    }
    pub(in crate::analysis) fn provenance(&self) -> &SymbolCallProvenance {
        &self.provenance
    }
    pub(in crate::analysis) fn static_string(&self) -> Option<&str> {
        self.static_string.as_deref()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FunctionEffects {
    by_id: FunctionTable<FunctionEffect>,
    /// Local effect limits fail closed and are surfaced as a project
    /// diagnostic instead of looking like a clean analysis.
    budget_exhausted: bool,
}

impl FunctionEffects {
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&FunctionEffect> {
        self.by_id.get(id)
    }

    pub(in crate::analysis) fn iter_effects(&self) -> impl Iterator<Item = &FunctionEffect> {
        self.by_id.values()
    }

    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(in crate::analysis) fn collect(stream: &FactStream) -> Self {
        let mut effects = Self::default();
        let budget = BudgetTracker::default();
        let mut value_provenance = BTreeMap::new();
        effects.initialize(stream, &budget);

        for fact in stream.facts() {
            let Some(effect) = effects.by_id.get_mut(fact.function) else {
                continue;
            };
            match &fact.payload {
                FactPayload::Reference {
                    value,
                    static_string,
                    provenance,
                } => effect.record_reference(
                    *value,
                    static_string.as_ref(),
                    provenance,
                    &mut value_provenance,
                ),
                FactPayload::Declaration { target, source }
                | FactPayload::Assignment {
                    target,
                    source,
                    receiver: None,
                } => effect.record_copy(*target, *source),
                FactPayload::Assignment {
                    receiver: Some(_), ..
                } => effect.invalid = true,
                FactPayload::PropertyWrite {
                    receiver,
                    property,
                    static_value,
                    ..
                } => effect.record_property_write(
                    fact.id,
                    *receiver,
                    property.as_ref(),
                    static_value.as_ref(),
                    &budget,
                ),
                FactPayload::Call { .. } => effect.record_call(fact, &budget),
                FactPayload::Control {
                    kind: ControlKind::Return,
                    value,
                    ..
                } => effect.record_return(*value, &value_provenance, &budget),
                FactPayload::Control { kind, .. }
                    if !matches!(
                        kind,
                        ControlKind::BranchStart
                            | ControlKind::BranchThen
                            | ControlKind::BranchElse
                            | ControlKind::BranchEnd
                            | ControlKind::LoopStart { .. }
                            | ControlKind::LoopUpdate
                            | ControlKind::LoopEnd
                            | ControlKind::SwitchStart
                            | ControlKind::SwitchCase { .. }
                            | ControlKind::SwitchEnd
                            | ControlKind::TryStart
                            | ControlKind::CatchStart
                            | ControlKind::FinallyStart
                            | ControlKind::TryEnd
                            | ControlKind::Break
                            | ControlKind::Continue
                            | ControlKind::Return
                    ) => {}
                _ => {}
            }
            effect.mark_unsupported_control(&fact.payload);
        }
        effects.budget_exhausted = budget.is_exhausted();
        effects
    }

    fn initialize(&mut self, stream: &FactStream, budget: &BudgetTracker) {
        for fact in stream.facts() {
            let FactPayload::Function {
                id,
                parameters,
                boundary: crate::analysis::facts::FunctionBoundary::Enter,
                ..
            } = &fact.payload
            else {
                continue;
            };
            if !self.by_id.contains(*id) && self.by_id.len() >= MAX_FUNCTION_EFFECTS {
                mark_budget(budget);
                continue;
            }
            self.by_id.insert(
                *id,
                FunctionEffect {
                    id: *id,
                    parameters: parameters.clone(),
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots: parameters
                        .iter()
                        .map(|parameter| (parameter.value, parameter.value))
                        .collect(),
                },
            );
        }
        self.by_id.insert(
            FunctionId(0),
            FunctionEffect {
                id: FunctionId(0),
                parameters: Vec::new(),
                calls: Vec::new(),
                uses: Vec::new(),
                returns: Vec::new(),
                invalid: false,
                value_roots: BTreeMap::new(),
            },
        );
    }
}

fn mark_budget(budget: &BudgetTracker) {
    budget.mark_exhausted();
}

impl FunctionEffect {
    fn record_property_write(
        &mut self,
        event: super::super::facts::FactId,
        receiver: ValueId,
        property: Option<&String>,
        static_value: Option<&String>,
        budget: &BudgetTracker,
    ) {
        if self.uses.len() >= MAX_EFFECT_USES {
            self.invalid = true;
            mark_budget(budget);
            return;
        }
        self.uses.push(EffectUse::PropertyWrite {
            event,
            receiver: self.parameter_for(receiver),
            value: receiver,
            property: property.cloned(),
            static_value: static_value.cloned(),
        });
    }
}

impl FunctionEffect {
    pub(in crate::analysis) fn id(&self) -> FunctionId {
        self.id
    }
    pub(in crate::analysis) fn calls(&self) -> &[EffectCall] {
        &self.calls
    }
    pub(in crate::analysis) fn uses(&self) -> &[EffectUse] {
        &self.uses
    }
    pub(in crate::analysis) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }
    pub(in crate::analysis) fn returns(&self) -> &[ReturnProjection] {
        &self.returns
    }
    pub(in crate::analysis) fn is_invalid(&self) -> bool {
        self.invalid
    }
    pub(in crate::analysis) fn value_root(&self, value: ValueId) -> Option<ValueId> {
        self.value_roots.get(&value).copied()
    }

    fn mark_unsupported_control(&mut self, payload: &FactPayload) {
        // Unsupported control is deliberately conservative for effects. The
        // local projector still handles its precise single-file semantics.
        if matches!(
            payload,
            FactPayload::Control {
                kind: ControlKind::BranchStart
                    | ControlKind::LoopStart { .. }
                    | ControlKind::SwitchStart
                    | ControlKind::TryStart,
                ..
            }
        ) {
            self.invalid = true;
        }
    }

    fn record_call(&mut self, fact: &super::super::facts::SemanticFact, budget: &BudgetTracker) {
        let FactPayload::Call {
            syntactic_chain,
            rooted_chain,
            args,
            target_function,
            result,
            unwrap,
            call_provenance,
            receiver,
            ..
        } = &fact.payload
        else {
            return;
        };

        let (chain, call_args) = unwrap.as_deref().map_or_else(
            || {
                (
                    rooted_chain.clone().or_else(|| syntactic_chain.clone()),
                    args.as_slice(),
                )
            },
            |unwrap| (Some(unwrap.chain.clone()), unwrap.effective_args.as_slice()),
        );
        let arguments = call_args
            .iter()
            .enumerate()
            .map(|(index, argument)| EffectArgument {
                index,
                value: argument.base_value,
                path: argument.base_path,
                parameter: self.parameter_for(argument.base_value),
            })
            .collect::<Vec<_>>();
        if self.calls.len() < MAX_EFFECT_CALLS {
            self.calls.push(EffectCall {
                event: fact.id,
                chain: chain.clone(),
                rooted: rooted_chain.is_some(),
                target: *target_function,
                result: *result,
                provenance: call_provenance.clone(),
                arguments: arguments.clone(),
                call_arguments: call_args.to_vec(),
            });
        } else {
            self.invalid = true;
            mark_budget(budget);
        }
        if let Some(receiver) = receiver.and_then(|value| self.parameter_for(value)) {
            if self.uses.len() < MAX_EFFECT_USES {
                self.uses.push(EffectUse::CallReceiver {
                    event: fact.id,
                    chain: chain.clone(),
                    receiver,
                    call_arguments: call_args.to_vec(),
                });
            } else {
                self.invalid = true;
                mark_budget(budget);
            }
        }
        for argument in arguments {
            if self.uses.len() >= MAX_EFFECT_USES {
                self.invalid = true;
                mark_budget(budget);
                break;
            }
            self.uses.push(EffectUse::CallArgument {
                event: fact.id,
                chain: chain.clone(),
                rooted: rooted_chain.is_some(),
                argument,
            });
        }
        self.value_roots.entry(*result).or_insert(*result);
    }

    fn record_copy(&mut self, target: ValueId, source: ValueId) {
        self.copy_root(target, source);
        if self.parameter_for(source).is_some() {
            self.value_roots.insert(target, source);
        }
    }

    fn copy_root(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if source == ValueId::UNKNOWN {
            self.value_roots.remove(&target);
        } else {
            let root = self.value_roots.get(&source).copied().unwrap_or(source);
            self.value_roots.insert(target, root);
        }
    }

    fn parameter_for(&self, value: ValueId) -> Option<ParameterRef> {
        let root = self.value_roots.get(&value).copied().unwrap_or(value);
        self.parameters
            .iter()
            .find(|parameter| parameter.value == root && root != ValueId::UNKNOWN)
            .map(|parameter| ParameterRef {
                index: parameter.parameter_index,
                path: parameter.path,
            })
    }

    fn record_reference(
        &mut self,
        value: ValueId,
        static_string: Option<&String>,
        provenance: &SymbolCallProvenance,
        value_provenance: &mut BTreeMap<ValueId, (SymbolCallProvenance, Option<String>)>,
    ) {
        value_provenance.insert(value, (provenance.clone(), static_string.cloned()));
        if value != ValueId::UNKNOWN {
            self.value_roots.entry(value).or_insert(value);
        }
    }

    fn record_return(
        &mut self,
        value: ValueId,
        value_provenance: &BTreeMap<ValueId, (SymbolCallProvenance, Option<String>)>,
        budget: &BudgetTracker,
    ) {
        let parameter = self.parameter_for(value);
        if parameter.is_none()
            && (value == ValueId::UNKNOWN || !self.value_roots.contains_key(&value))
        {
            if value != ValueId::UNKNOWN {
                self.invalid = true;
            }
            return;
        }
        if self.returns.len() >= MAX_EFFECT_RETURNS {
            self.invalid = true;
            mark_budget(budget);
            return;
        }
        let provenance = value_provenance
            .get(&value)
            .map_or(SymbolCallProvenance::Local, |(provenance, _)| {
                provenance.clone()
            });
        let static_string = value_provenance
            .get(&value)
            .and_then(|(_, value)| value.clone());
        self.returns.push(ReturnProjection {
            value,
            parameter,
            provenance,
            static_string,
        });
    }
}
