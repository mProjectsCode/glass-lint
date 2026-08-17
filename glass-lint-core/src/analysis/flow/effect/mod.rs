//! Public-matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.
//!
//! An effect becomes invalid when unsupported control flow or an effect budget
//! prevents a complete summary. Invalid summaries are not used for qualified
//! propagation, preserving fail-closed behavior across module boundaries.

mod domain;
pub(in crate::analysis) use domain::*;
use glass_lint_datastructures::Budget;
use hashbrown::HashMap;
use smol_str::SmolStr;

use crate::analysis::{
    DerivedPhaseAvailability,
    facts::{
        CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        ParameterBinding, SemanticFact,
    },
    flow::{FlowCompletion, FlowCompletionReason},
    model::{flow::FunctionTable, scope::FunctionId, value::ValueId},
    syntax::SymbolCallProvenance,
};

#[derive(Clone, Debug)]
pub(in crate::analysis) struct FunctionEffect {
    id: FunctionId,
    calls: Vec<EffectCall>,
    uses: Vec<EffectUse>,
    returns: Vec<ReturnProjection>,
    invalid: bool,
    value_roots: HashMap<ValueId, ValueId>,
    parameter_index: HashMap<ValueId, ParameterRef>,
}

impl FunctionEffect {
    fn empty(id: FunctionId) -> Self {
        Self {
            id,
            calls: Vec::new(),
            uses: Vec::new(),
            returns: Vec::new(),
            invalid: false,
            value_roots: HashMap::new(),
            parameter_index: HashMap::new(),
        }
    }

    fn invalid(id: FunctionId) -> Self {
        Self {
            invalid: true,
            ..Self::empty(id)
        }
    }

    fn with_parameters(id: FunctionId, parameters: &[ParameterBinding]) -> Self {
        Self {
            value_roots: parameters
                .iter()
                .map(|parameter| (parameter.value(), parameter.value()))
                .collect(),
            parameter_index: parameters
                .iter()
                .map(|parameter| {
                    (
                        parameter.value(),
                        ParameterRef {
                            index: parameter.parameter_index(),
                            path: parameter.path(),
                        },
                    )
                })
                .collect(),
            ..Self::empty(id)
        }
    }

    fn record_property_write(
        &mut self,
        event: FactId,
        receiver: ValueId,
        property: Option<&str>,
        value_is_precise: bool,
        budget: &mut Budget,
    ) {
        if !budget.try_push() {
            self.invalid = true;
            return;
        }
        self.uses.push(EffectUse::PropertyWrite {
            event,
            receiver: self.parameter_for(receiver),
            receiver_value: receiver,
            property: property.map(SmolStr::new),
            value_is_precise,
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
    ) -> Option<&'s [ParameterBinding]> {
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

    /// Canonical root for `value`, owned by the record paths. An unknown root
    /// is the value itself.
    fn root_of(&self, value: ValueId) -> ValueId {
        self.value_root(value).unwrap_or(value)
    }

    /// Totalized root lookup for cross-phase consumers: an unknown root is the
    /// value itself.
    pub(in crate::analysis) fn root_value(&self, value: ValueId) -> ValueId {
        self.root_of(value)
    }

    pub(in crate::analysis) fn call_argument(
        &self,
        call_id: EffectCallId,
        index: usize,
    ) -> Option<&EffectArgument> {
        self.calls
            .get(call_id.index())
            .and_then(|call| call.arguments().get(index))
    }

    fn mark_unsupported_control(&mut self, payload: &FactPayload) {
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

    fn record_call(&mut self, fact: &SemanticFact, budget: &mut Budget) {
        let FactPayload::Call(call) = fact.payload() else {
            return;
        };

        let effective_args = call.effective_args();
        let arguments = self.build_effect_arguments(effective_args);
        let call_id = EffectCallId::new(self.calls.len());
        for argument in &arguments {
            if !budget.try_push() {
                self.invalid = true;
                return;
            }
            self.uses.push(EffectUse::CallArgument {
                call_id,
                event: fact.id(),
                argument_index: argument.index(),
            });
        }
        if budget.try_push() {
            self.calls.push(EffectCall {
                event: fact.id(),
                arguments,
            });
        } else {
            self.invalid = true;
        }
        if let Some(receiver) = call.receiver().and_then(|value| self.parameter_for(value)) {
            if budget.try_push() {
                self.uses.push(EffectUse::CallReceiver {
                    event: fact.id(),
                    receiver,
                });
            } else {
                self.invalid = true;
            }
        }
        self.value_roots
            .entry(call.result())
            .or_insert(call.result());
    }

    fn build_effect_arguments(&self, call_args: &[CallArgInfo]) -> Vec<EffectArgument> {
        call_args
            .iter()
            .enumerate()
            .map(|(index, argument)| EffectArgument {
                index,
                value: argument.base_value,
                path: argument.base_path,
                parameter: self.parameter_for(argument.base_value),
            })
            .collect()
    }

    fn record_copy(&mut self, target: ValueId, source: ValueId) {
        self.copy_root(target, source);
    }

    fn copy_root(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if source == ValueId::UNKNOWN {
            if !self.parameter_index.contains_key(&target) {
                self.value_roots.remove(&target);
            }
        } else {
            let root = self.root_of(source);
            self.value_roots.insert(target, root);
        }
    }

    fn parameter_for(&self, value: ValueId) -> Option<ParameterRef> {
        let root = self.root_of(value);
        if root == ValueId::UNKNOWN {
            return None;
        }
        self.parameter_index.get(&root).cloned()
    }

    fn record_reference(
        &mut self,
        value: ValueId,
        provenance: &SymbolCallProvenance,
        value_provenance: &mut HashMap<ValueId, SymbolCallProvenance>,
    ) {
        value_provenance.insert(value, provenance.clone());
        if value != ValueId::UNKNOWN {
            self.value_roots.entry(value).or_insert(value);
        }
    }

    fn record_return(
        &mut self,
        value: ValueId,
        value_provenance: &HashMap<ValueId, SymbolCallProvenance>,
        budget: &mut Budget,
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

#[derive(Clone, Debug)]
pub(in crate::analysis) struct FunctionEffects {
    by_id: FunctionTable<FunctionEffect>,
    completion: FlowCompletion,
    operation_count: usize,
    availability: DerivedPhaseAvailability,
}

impl Default for FunctionEffects {
    fn default() -> Self {
        Self {
            by_id: FunctionTable::new(0),
            completion: FlowCompletion::default(),
            operation_count: 0,
            availability: DerivedPhaseAvailability::Enabled,
        }
    }
}

impl FunctionEffects {
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&FunctionEffect> {
        self.by_id.get(id)
    }

    pub(in crate::analysis) fn iter_effects(&self) -> impl Iterator<Item = &FunctionEffect> {
        self.by_id.values()
    }

    pub(in crate::analysis) fn completion(&self) -> FlowCompletion {
        self.completion
    }

    pub(in crate::analysis) fn operation_count(&self) -> usize {
        self.operation_count
    }

    pub(in crate::analysis) fn is_available(&self) -> bool {
        self.availability.is_enabled()
    }

    #[cfg(test)]
    pub(in crate::analysis) fn collect(stream: &FactStream<Frozen>, limit: usize) -> Self {
        Self::collect_with_availability(stream, limit, DerivedPhaseAvailability::Enabled)
    }

    pub(in crate::analysis) fn collect_with_availability(
        stream: &FactStream<Frozen>,
        limit: usize,
        availability: DerivedPhaseAvailability,
    ) -> Self {
        let mut builder = FunctionEffectsBuilder::new(stream, limit, availability);
        for fact in stream.facts() {
            builder.consume(fact);
        }
        builder.finish()
    }
}

/// Mutable construction state for function effects.
///
/// Semantic analysis can feed this builder while it projects the same frozen
/// fact tape into occurrence indexes. Keeping construction separate from the
/// immutable `FunctionEffects` value makes the shared derived pass explicit
/// without exposing either consumer's storage.
pub(in crate::analysis) struct FunctionEffectsBuilder<'stream> {
    stream: &'stream FactStream<Frozen>,
    by_id: FunctionTable<FunctionEffect>,
    budget: Budget,
    value_provenance: HashMap<ValueId, SymbolCallProvenance>,
    availability: DerivedPhaseAvailability,
}

impl<'stream> FunctionEffectsBuilder<'stream> {
    pub(in crate::analysis) fn new(
        stream: &'stream FactStream<Frozen>,
        limit: usize,
        availability: DerivedPhaseAvailability,
    ) -> Self {
        let mut builder = Self {
            stream,
            by_id: FunctionTable::new(stream.function_count()),
            budget: Budget::new(limit),
            value_provenance: HashMap::new(),
            availability,
        };
        if builder.availability.is_enabled() && builder.budget.try_push() {
            let _ = builder.by_id.insert(
                FunctionId::new(0),
                FunctionEffect::empty(FunctionId::new(0)),
            );
        }
        builder
    }

    pub(in crate::analysis) fn consume(&mut self, fact: &SemanticFact) {
        if !self.availability.is_enabled() {
            return;
        }
        if let FactPayload::Function {
            id,
            boundary: FunctionBoundary::Enter,
            ..
        } = fact.payload()
        {
            if !self.by_id.contains(*id) && !self.budget.try_push() {
                return;
            }
            let Some(params) = self.stream.function_parameters(*id) else {
                let _ = self.by_id.insert(*id, FunctionEffect::invalid(*id));
                return;
            };
            let _ = self
                .by_id
                .insert(*id, FunctionEffect::with_parameters(*id, params));
            return;
        }

        let Some(effect) = self.by_id.get_mut(fact.function()) else {
            return;
        };
        match fact.payload() {
            FactPayload::Reference {
                value, provenance, ..
            } => {
                effect.record_reference(*value, provenance, &mut self.value_provenance);
            }
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
                value: _,
                rooted_chain: _,
                value_is_precise,
            } => effect.record_property_write(
                fact.id(),
                *receiver,
                property.and_then(|id| self.stream.resolve_name(id)),
                *value_is_precise,
                &mut self.budget,
            ),
            FactPayload::Call(_) => effect.record_call(fact, &mut self.budget),
            FactPayload::Return { value } => {
                effect.record_return(*value, &self.value_provenance, &mut self.budget);
            }
            _ => {}
        }
        effect.mark_unsupported_control(fact.payload());
    }

    pub(in crate::analysis) fn finish(self) -> FunctionEffects {
        if !self.availability.is_enabled() {
            return FunctionEffects {
                by_id: FunctionTable::new(0),
                completion: FlowCompletion::incomplete(FlowCompletionReason::PhaseDisabled),
                operation_count: 0,
                availability: self.availability,
            };
        }
        FunctionEffects {
            by_id: self.by_id,
            completion: if self.budget.exhausted() {
                FlowCompletion::incomplete(FlowCompletionReason::EffectBudget)
            } else {
                FlowCompletion::default()
            },
            operation_count: self.budget.used(),
            availability: self.availability,
        }
    }
}

#[cfg(test)]
mod tests;
