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
    pub(in crate::analysis) by_id: BTreeMap<FunctionId, FunctionEffect>,
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

pub(in crate::analysis) fn collect(stream: &FactStream) -> FunctionEffects {
    let mut effects = FunctionEffects::default();
    let mut budget_exhausted = false;
    let mut value_provenance = BTreeMap::new();
    initialize_effects(stream, &mut effects, &mut budget_exhausted);

    for fact in stream.facts() {
        let Some(effect) = effects.by_id.get_mut(&fact.function) else {
            continue;
        };
        match &fact.payload {
            FactPayload::Reference {
                value,
                static_string,
                provenance,
            } => record_reference(
                effect,
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
            } => record_copy(effect, *target, *source),
            FactPayload::Assignment {
                receiver: Some(_), ..
            } => effect.invalid = true,
            FactPayload::PropertyWrite {
                receiver,
                property,
                static_value,
                ..
            } => record_property_write(
                effect,
                fact.id,
                *receiver,
                property.as_ref(),
                static_value.as_ref(),
                &mut budget_exhausted,
            ),
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
            } => record_call(CallInput {
                effect,
                event: fact.id,
                syntactic_chain: syntactic_chain.as_ref(),
                rooted_chain: rooted_chain.as_ref(),
                args,
                target_function: *target_function,
                result: *result,
                unwrap: unwrap.as_deref(),
                call_provenance,
                receiver: *receiver,
                budget_exhausted: &mut budget_exhausted,
            }),
            FactPayload::Control {
                kind: ControlKind::Return,
                value,
                ..
            } => record_return(effect, *value, &value_provenance, &mut budget_exhausted),
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
        mark_unsupported_control(effect, &fact.payload);
    }
    effects.budget_exhausted = budget_exhausted;
    effects
}

fn record_property_write(
    effect: &mut FunctionEffect,
    event: super::super::facts::FactId,
    receiver: ValueId,
    property: Option<&String>,
    static_value: Option<&String>,
    budget_exhausted: &mut bool,
) {
    if effect.uses.len() >= MAX_EFFECT_USES {
        effect.invalid = true;
        mark_budget(budget_exhausted);
        return;
    }
    effect.uses.push(EffectUse::PropertyWrite {
        event,
        receiver: relation(effect, receiver),
        value: receiver,
        property: property.cloned(),
        static_value: static_value.cloned(),
    });
}

fn mark_unsupported_control(effect: &mut FunctionEffect, payload: &FactPayload) {
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
        effect.invalid = true;
    }
}

struct CallInput<'a> {
    effect: &'a mut FunctionEffect,
    event: super::super::facts::FactId,
    syntactic_chain: Option<&'a String>,
    rooted_chain: Option<&'a String>,
    args: &'a [CallArgInfo],
    target_function: Option<FunctionId>,
    result: ValueId,
    unwrap: Option<&'a super::super::facts::CallUnwrap>,
    call_provenance: &'a SymbolCallProvenance,
    receiver: Option<ValueId>,
    budget_exhausted: &'a mut bool,
}

fn initialize_effects(
    stream: &FactStream,
    effects: &mut FunctionEffects,
    budget_exhausted: &mut bool,
) {
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
        if !effects.by_id.contains_key(id) && effects.by_id.len() >= MAX_FUNCTION_EFFECTS {
            mark_budget(budget_exhausted);
            continue;
        }
        effects.by_id.entry(*id).or_insert_with(|| FunctionEffect {
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
        });
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
}

fn record_reference(
    effect: &mut FunctionEffect,
    value: ValueId,
    static_string: Option<&String>,
    provenance: &SymbolCallProvenance,
    value_provenance: &mut BTreeMap<ValueId, (SymbolCallProvenance, Option<String>)>,
) {
    value_provenance.insert(value, (provenance.clone(), static_string.cloned()));
    if value != ValueId::UNKNOWN {
        effect.value_roots.entry(value).or_insert(value);
    }
}

fn record_copy(effect: &mut FunctionEffect, target: ValueId, source: ValueId) {
    copy_root(effect, target, source);
    if relation(effect, source).is_some() {
        effect.value_roots.insert(target, source);
    }
}

fn record_call(input: CallInput<'_>) {
    let CallInput {
        effect,
        event,
        syntactic_chain,
        rooted_chain,
        args,
        target_function,
        result,
        unwrap,
        call_provenance,
        receiver,
        budget_exhausted,
    } = input;
    let (chain, call_args) = unwrap.map_or_else(
        || {
            (
                rooted_chain.cloned().or_else(|| syntactic_chain.cloned()),
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
            event,
            chain: chain.clone(),
            rooted: rooted_chain.is_some(),
            target: target_function,
            result,
            provenance: call_provenance.clone(),
            arguments: arguments.clone(),
            call_arguments: call_args.to_vec(),
        });
    } else {
        effect.invalid = true;
        mark_budget(budget_exhausted);
    }
    if let Some(receiver) = receiver.and_then(|value| relation(effect, value)) {
        if effect.uses.len() < MAX_EFFECT_USES {
            effect.uses.push(EffectUse::CallReceiver {
                event,
                chain: chain.clone(),
                receiver,
                call_arguments: call_args.to_vec(),
            });
        } else {
            effect.invalid = true;
            mark_budget(budget_exhausted);
        }
    }
    for argument in arguments {
        if effect.uses.len() >= MAX_EFFECT_USES {
            effect.invalid = true;
            mark_budget(budget_exhausted);
            break;
        }
        effect.uses.push(EffectUse::CallArgument {
            event,
            chain: chain.clone(),
            rooted: rooted_chain.is_some(),
            argument,
        });
    }
    effect.value_roots.entry(result).or_insert(result);
}

fn record_return(
    effect: &mut FunctionEffect,
    value: ValueId,
    value_provenance: &BTreeMap<ValueId, (SymbolCallProvenance, Option<String>)>,
    budget_exhausted: &mut bool,
) {
    let parameter = relation(effect, value);
    if parameter.is_none()
        && (value == ValueId::UNKNOWN || !effect.value_roots.contains_key(&value))
    {
        if value != ValueId::UNKNOWN {
            effect.invalid = true;
        }
        return;
    }
    if effect.returns.len() >= MAX_EFFECT_RETURNS {
        effect.invalid = true;
        mark_budget(budget_exhausted);
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
    effect.returns.push(ReturnProjection {
        value,
        parameter,
        provenance,
        static_string,
    });
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
