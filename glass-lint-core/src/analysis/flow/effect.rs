//! Matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.

use std::collections::BTreeMap;

use super::super::facts::{CallArgInfo, ControlKind, FactPayload, FactStream, ParameterBinding};
use super::super::syntax::SymbolCallProvenance;
use super::super::value::{FunctionId, PathId, ValueId};

const MAX_FUNCTION_EFFECTS: usize = 65_536;
const MAX_EFFECT_CALLS: usize = 65_536;
const MAX_EFFECT_USES: usize = 131_072;
const MAX_EFFECT_RETURNS: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct ParameterRef {
    pub(super) index: usize,
    pub(super) path: PathId,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectArgument {
    pub(super) index: usize,
    pub(super) value: ValueId,
    pub(super) path: PathId,
    pub(super) parameter: Option<ParameterRef>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectCall {
    pub(in crate::analysis) event: super::super::facts::FactId,
    pub(in crate::analysis) chain: Option<String>,
    pub(in crate::analysis) rooted: bool,
    pub(in crate::analysis) target: Option<FunctionId>,
    pub(in crate::analysis) result: ValueId,
    pub(in crate::analysis) provenance: SymbolCallProvenance,
    pub(in crate::analysis) arguments: Vec<EffectArgument>,
    pub(in crate::analysis) call_arguments: Vec<CallArgInfo>,
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
    pub(in crate::analysis) id: FunctionId,
    pub(in crate::analysis) parameters: Vec<ParameterBinding>,
    pub(in crate::analysis) calls: Vec<EffectCall>,
    pub(in crate::analysis) uses: Vec<EffectUse>,
    pub(in crate::analysis) returns: Vec<ReturnProjection>,
    pub(in crate::analysis) invalid: bool,
    /// Source-order value copies. Project flow uses this to connect a source
    /// call result through local declarations before a qualified call.
    pub(in crate::analysis) value_roots: BTreeMap<ValueId, ValueId>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct ReturnProjection {
    pub(in crate::analysis) value: ValueId,
    pub(in crate::analysis) parameter: Option<ParameterRef>,
    pub(in crate::analysis) provenance: SymbolCallProvenance,
    pub(in crate::analysis) static_string: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FunctionEffects {
    pub(crate) by_id: BTreeMap<FunctionId, FunctionEffect>,
    /// Local effect limits fail closed and are surfaced as a project
    /// diagnostic instead of looking like a clean analysis.
    pub(crate) budget_exhausted: bool,
}

impl FunctionEffects {
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&FunctionEffect> {
        self.by_id.get(&id)
    }
}

fn mark_budget(exhausted: &mut bool) {
    *exhausted = true;
}

#[allow(clippy::too_many_lines)]
pub(in crate::analysis) fn collect(stream: &FactStream) -> FunctionEffects {
    let mut effects = FunctionEffects::default();
    let mut budget_exhausted = false;
    let mut value_provenance = BTreeMap::new();
    for fact in stream.facts() {
        if let FactPayload::Function {
            id,
            parameters,
            boundary: crate::analysis::facts::FunctionBoundary::Enter,
            ..
        } = &fact.payload
        {
            if !effects.by_id.contains_key(id) && effects.by_id.len() >= MAX_FUNCTION_EFFECTS {
                mark_budget(&mut budget_exhausted);
                continue;
            }
            effects.by_id.entry(*id).or_insert_with(|| {
                let mut value_roots = BTreeMap::new();
                for parameter in parameters {
                    value_roots.insert(parameter.value, parameter.value);
                }
                FunctionEffect {
                    id: *id,
                    parameters: parameters.clone(),
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots,
                }
            });
        }
    }
    effects
        .by_id
        .entry(FunctionId(0))
        .or_insert_with(|| FunctionEffect {
            id: FunctionId(0),
            parameters: Vec::new(),
            calls: Vec::new(),
            uses: Vec::new(),
            returns: Vec::new(),
            invalid: false,
            value_roots: BTreeMap::new(),
        });

    for fact in stream.facts() {
        let Some(effect) = effects.by_id.get_mut(&fact.function) else {
            continue;
        };
        match &fact.payload {
            FactPayload::Reference {
                value,
                static_string,
                provenance,
            } => {
                value_provenance.insert(*value, (provenance.clone(), static_string.clone()));
                if *value != ValueId::UNKNOWN {
                    effect.value_roots.entry(*value).or_insert(*value);
                }
            }
            FactPayload::Declaration { target, source }
            | FactPayload::Assignment {
                target,
                source,
                receiver: None,
            } => {
                copy_root(effect, *target, *source);
                if let Some(parameter) = relation(effect, *source) {
                    effect.value_roots.insert(*target, *source);
                    // The parameter relation is recovered from the copied
                    // value by `relation`; no matcher-specific state leaks in.
                    let _ = parameter;
                }
            }
            FactPayload::Assignment {
                receiver: Some(_), ..
            } => effect.invalid = true,
            FactPayload::PropertyWrite {
                receiver,
                property,
                static_value,
                ..
            } => {
                if let Some(parameter) = relation(effect, *receiver) {
                    if effect.uses.len() < MAX_EFFECT_USES {
                        effect.uses.push(EffectUse::PropertyWrite {
                            event: fact.id,
                            receiver: Some(parameter),
                            value: *receiver,
                            property: property.clone(),
                            static_value: static_value.clone(),
                        });
                    } else {
                        effect.invalid = true;
                        mark_budget(&mut budget_exhausted);
                    }
                } else {
                    if effect.uses.len() < MAX_EFFECT_USES {
                        effect.uses.push(EffectUse::PropertyWrite {
                            event: fact.id,
                            receiver: None,
                            value: *receiver,
                            property: property.clone(),
                            static_value: static_value.clone(),
                        });
                    } else {
                        effect.invalid = true;
                        mark_budget(&mut budget_exhausted);
                    }
                }
            }
            FactPayload::Call {
                syntactic_chain,
                rooted_chain,
                args,
                target_function,
                result,
                unwrap,
                call_provenance,
                receiver,
                ..
            } => {
                let (chain, call_args) = unwrap.as_deref().map_or_else(
                    || {
                        (
                            rooted_chain.clone().or_else(|| syntactic_chain.clone()),
                            args,
                        )
                    },
                    |unwrap| (Some(unwrap.chain.clone()), &unwrap.effective_args),
                );
                let arguments = call_args
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| EffectArgument {
                        index,
                        value: argument.base_value,
                        path: argument.base_path,
                        parameter: relation(effect, argument.base_value),
                    })
                    .collect::<Vec<_>>();
                if effect.calls.len() < MAX_EFFECT_CALLS {
                    effect.calls.push(EffectCall {
                        event: fact.id,
                        chain: chain.clone(),
                        rooted: rooted_chain.is_some(),
                        target: *target_function,
                        result: *result,
                        provenance: call_provenance.clone(),
                        arguments: arguments.clone(),
                        call_arguments: call_args.clone(),
                    });
                } else {
                    effect.invalid = true;
                    mark_budget(&mut budget_exhausted);
                }
                if let Some(receiver) = receiver.and_then(|value| relation(effect, value)) {
                    if effect.uses.len() < MAX_EFFECT_USES {
                        effect.uses.push(EffectUse::CallReceiver {
                            event: fact.id,
                            chain: chain.clone(),
                            receiver,
                            call_arguments: call_args.clone(),
                        });
                    } else {
                        effect.invalid = true;
                        mark_budget(&mut budget_exhausted);
                    }
                }
                for argument in arguments {
                    if effect.uses.len() >= MAX_EFFECT_USES {
                        effect.invalid = true;
                        mark_budget(&mut budget_exhausted);
                        break;
                    }
                    effect.uses.push(EffectUse::CallArgument {
                        event: fact.id,
                        chain: chain.clone(),
                        rooted: rooted_chain.is_some(),
                        argument,
                    });
                }
                effect.value_roots.entry(*result).or_insert(*result);
            }
            FactPayload::Control {
                kind: ControlKind::Return,
                value,
                ..
            } => {
                if let Some(parameter) = relation(effect, *value) {
                    if effect.returns.len() < MAX_EFFECT_RETURNS {
                        effect.returns.push(ReturnProjection {
                            value: *value,
                            parameter: Some(parameter),
                            provenance: value_provenance
                                .get(value)
                                .map_or(SymbolCallProvenance::Local, |(provenance, _)| {
                                    provenance.clone()
                                }),
                            static_string: value_provenance
                                .get(value)
                                .and_then(|(_, value)| value.clone()),
                        });
                    } else {
                        effect.invalid = true;
                        mark_budget(&mut budget_exhausted);
                    }
                } else if *value != ValueId::UNKNOWN {
                    if effect.value_roots.contains_key(value) {
                        if effect.returns.len() < MAX_EFFECT_RETURNS {
                            effect.returns.push(ReturnProjection {
                                value: *value,
                                parameter: None,
                                provenance: value_provenance
                                    .get(value)
                                    .map_or(SymbolCallProvenance::Local, |(provenance, _)| {
                                        provenance.clone()
                                    }),
                                static_string: value_provenance
                                    .get(value)
                                    .and_then(|(_, value)| value.clone()),
                            });
                        } else {
                            effect.invalid = true;
                            mark_budget(&mut budget_exhausted);
                        }
                    } else {
                        effect.invalid = true;
                    }
                }
            }
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
        // Unsupported control is deliberately conservative for effects. The
        // local projector still handles its precise single-file semantics.
        if matches!(
            &fact.payload,
            FactPayload::Control {
                kind: ControlKind::BranchStart
                    | ControlKind::LoopStart { .. }
                    | ControlKind::SwitchStart
                    | ControlKind::TryStart,
                ..
            }
        ) {
            effect.invalid = true;
        }
    }
    effects.budget_exhausted = budget_exhausted;
    effects
}

fn copy_root(effect: &mut FunctionEffect, target: ValueId, source: ValueId) {
    if target == ValueId::UNKNOWN {
        return;
    }
    if source == ValueId::UNKNOWN {
        effect.value_roots.remove(&target);
    } else {
        let root = effect.value_roots.get(&source).copied().unwrap_or(source);
        effect.value_roots.insert(target, root);
    }
}

fn relation(effect: &FunctionEffect, value: ValueId) -> Option<ParameterRef> {
    let root = effect.value_roots.get(&value).copied().unwrap_or(value);
    effect
        .parameters
        .iter()
        .find(|parameter| parameter.value == root && root != ValueId::UNKNOWN)
        .map(|parameter| ParameterRef {
            index: parameter.parameter_index,
            path: parameter.path,
        })
}

#[allow(dead_code)]
fn _argument_value(argument: &CallArgInfo) -> ValueId {
    argument.base_value
}
