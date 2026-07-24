use std::collections::BTreeMap;

use glass_lint_datastructures::Budget;
use smol_str::SmolStr;

use super::types::{
    EffectArgument, EffectCall, EffectCallId, EffectUse, ParameterRef, ReturnProjection,
};
use crate::analysis::{
    facts::{
        CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, ParameterBinding,
        SemanticFact,
    },
    syntax::SymbolCallProvenance,
    value::{FunctionId, ValueId},
};

#[derive(Clone, Debug)]
pub struct FunctionEffect {
    pub(super) id: FunctionId,
    pub(super) calls: Vec<EffectCall>,
    pub(super) uses: Vec<EffectUse>,
    pub(super) returns: Vec<ReturnProjection>,
    pub(super) invalid: bool,
    pub(super) value_roots: BTreeMap<ValueId, ValueId>,
    pub(super) parameter_index: BTreeMap<ValueId, ParameterRef>,
}

impl FunctionEffect {
    pub(super) fn record_property_write(
        &mut self,
        event: FactId,
        receiver: ValueId,
        property: Option<&str>,
        stream: &FactStream<Frozen>,
        budget: &mut Budget,
    ) {
        if !budget.try_push() {
            self.invalid = true;
            return;
        }
        self.uses.push(EffectUse::PropertyWrite {
            event,
            receiver: self.parameter_for(receiver, stream),
            receiver_value: receiver,
            property: property.map(SmolStr::new),
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

    pub(in crate::analysis) fn parameters<'s>(
        &self,
        stream: &'s FactStream<Frozen>,
    ) -> &'s [ParameterBinding] {
        stream.function_parameters(self.id)
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

    pub(in crate::analysis) fn call_argument(
        &self,
        call_id: EffectCallId,
        index: usize,
    ) -> Option<&EffectArgument> {
        self.calls
            .get(call_id.0)
            .and_then(|call| call.arguments().get(index))
    }

    pub(super) fn mark_unsupported_control(&mut self, payload: &FactPayload) {
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

    pub(super) fn record_call(
        &mut self,
        fact: &SemanticFact,
        stream: &FactStream<Frozen>,
        budget: &mut Budget,
    ) {
        let FactPayload::Call {
            args,
            result,
            unwrap,
            receiver,
            ..
        } = &fact.payload
        else {
            return;
        };

        let effective_args = unwrap
            .as_deref()
            .map_or(args.as_slice(), |u| u.effective_args.as_slice());
        let arguments = self.build_effect_arguments(effective_args, stream);
        let call_id = EffectCallId(self.calls.len());
        for argument in &arguments {
            if !budget.try_push() {
                self.invalid = true;
                return;
            }
            self.uses.push(EffectUse::CallArgument {
                call_id,
                event: fact.id,
                argument_index: argument.index(),
            });
        }
        if budget.try_push() {
            self.calls.push(EffectCall {
                event: fact.id,
                arguments,
            });
        } else {
            self.invalid = true;
        }
        if let Some(receiver) = receiver.and_then(|value| self.parameter_for(value, stream)) {
            if budget.try_push() {
                self.uses.push(EffectUse::CallReceiver {
                    event: fact.id,
                    receiver,
                });
            } else {
                self.invalid = true;
            }
        }
        self.value_roots.entry(*result).or_insert(*result);
    }

    fn build_effect_arguments(
        &self,
        call_args: &[CallArgInfo],
        stream: &FactStream<Frozen>,
    ) -> Vec<EffectArgument> {
        call_args
            .iter()
            .enumerate()
            .map(|(index, argument)| EffectArgument {
                index,
                value: argument.base_value,
                path: argument.base_path,
                parameter: self.parameter_for(argument.base_value, stream),
            })
            .collect()
    }

    pub(super) fn record_copy(&mut self, target: ValueId, source: ValueId) {
        self.copy_root(target, source);
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

    fn parameter_for(&self, value: ValueId, _stream: &FactStream<Frozen>) -> Option<ParameterRef> {
        let root = self.value_roots.get(&value).copied().unwrap_or(value);
        if root == ValueId::UNKNOWN {
            return None;
        }
        self.parameter_index.get(&root).cloned()
    }

    pub(super) fn record_reference(
        &mut self,
        value: ValueId,
        provenance: &SymbolCallProvenance,
        value_provenance: &mut BTreeMap<ValueId, SymbolCallProvenance>,
    ) {
        value_provenance.insert(value, provenance.clone());
        if value != ValueId::UNKNOWN {
            self.value_roots.entry(value).or_insert(value);
        }
    }

    pub(super) fn record_return(
        &mut self,
        value: ValueId,
        value_provenance: &BTreeMap<ValueId, SymbolCallProvenance>,
        stream: &FactStream<Frozen>,
        budget: &mut Budget,
    ) {
        let parameter = self.parameter_for(value, stream);
        if parameter.is_none()
            && (value == ValueId::UNKNOWN || !self.value_roots.contains_key(&value))
        {
            if value != ValueId::UNKNOWN {
                self.invalid = true;
            }
            return;
        }
        if !budget.try_push() {
            self.invalid = true;
            return;
        }
        let provenance = value_provenance
            .get(&value)
            .cloned()
            .unwrap_or(SymbolCallProvenance::Local);
        self.returns.push(ReturnProjection {
            value,
            parameter,
            provenance,
        });
    }
}
