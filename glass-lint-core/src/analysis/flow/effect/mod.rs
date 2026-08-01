//! Public-matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.
//!
//! An effect becomes invalid when unsupported control flow or an effect budget
//! prevents a complete summary. Invalid summaries are not used for qualified
//! propagation, preserving fail-closed behavior across module boundaries.

use std::borrow::Cow;

use glass_lint_datastructures::{Budget, NamePath, NameTable, PathId, SymbolPath};
use hashbrown::HashMap;
use smol_str::SmolStr;

use crate::analysis::{
    facts::{
        CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        ParameterBinding, SemanticFact,
    },
    model::flow::FunctionTable,
    syntax::SymbolCallProvenance,
    value::{FunctionId, ValueId},
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct EffectCallId(pub(in crate::analysis) usize);

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectCall {
    event: FactId,
    arguments: Vec<EffectArgument>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) enum EffectUse {
    PropertyWrite {
        event: FactId,
        receiver: Option<ParameterRef>,
        receiver_value: ValueId,
        property: Option<SmolStr>,
        value_is_precise: bool,
    },
    CallArgument {
        call_id: EffectCallId,
        event: FactId,
        argument_index: usize,
    },
    CallReceiver {
        event: FactId,
        receiver: ParameterRef,
    },
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct ReturnProjection {
    value: ValueId,
    parameter: Option<ParameterRef>,
    provenance: SymbolCallProvenance,
}

#[derive(Clone, Copy)]
pub(in crate::analysis) struct CallEffectRef<'stream> {
    pub(in crate::analysis) stream: &'stream FactStream<Frozen>,
    pub(in crate::analysis) event: FactId,
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
    pub(in crate::analysis) fn event(&self) -> FactId {
        self.event
    }

    pub(in crate::analysis) fn arguments(&self) -> &[EffectArgument] {
        &self.arguments
    }

    pub(in crate::analysis) fn as_ref<'s>(
        &'s self,
        stream: &'s FactStream<Frozen>,
    ) -> CallEffectRef<'s> {
        CallEffectRef {
            stream,
            event: self.event,
        }
    }
}

impl CallEffectRef<'_> {
    pub(super) fn call_fact(&self) -> Option<&FactPayload> {
        self.stream.fact(self.event).map(|fact| &fact.payload)
    }

    pub(in crate::analysis) fn chain(&self) -> Option<&NamePath> {
        match self.call_fact()? {
            FactPayload::Call {
                rooted_chain,
                syntactic_path,
                unwrap,
                ..
            } => unwrap
                .as_deref()
                .and_then(|u| u.chain_path.as_ref())
                .or(rooted_chain.as_ref())
                .or(syntactic_path.as_ref()),
            _ => None,
        }
    }

    pub(in crate::analysis) fn chain_owned(&self, names: &NameTable) -> Option<Cow<'_, NamePath>> {
        match self.call_fact()? {
            FactPayload::Call {
                rooted_chain,
                syntactic_path,
                callee_name,
                unwrap,
                ..
            } => unwrap
                .as_deref()
                .and_then(|u| u.chain_path.as_ref())
                .map(Cow::Borrowed)
                .or_else(|| rooted_chain.as_ref().map(Cow::Borrowed))
                .or_else(|| syntactic_path.as_ref().map(Cow::Borrowed))
                .or_else(|| {
                    callee_name
                        .and_then(|id| self.stream.resolve_name(id))
                        .and_then(|name| names.lookup_path(&SymbolPath::from(name)))
                        .map(Cow::Owned)
                }),
            _ => None,
        }
    }

    pub(in crate::analysis) fn rooted(&self) -> bool {
        self.call_fact().is_some_and(|fact| {
            matches!(
                fact,
                FactPayload::Call {
                    rooted_chain: Some(_),
                    ..
                }
            )
        })
    }

    pub(in crate::analysis) fn result(&self) -> ValueId {
        match self.call_fact() {
            Some(FactPayload::Call { result, .. }) => *result,
            _ => ValueId::UNKNOWN,
        }
    }

    pub(in crate::analysis) fn provenance(&self) -> Option<&SymbolCallProvenance> {
        match self.call_fact() {
            Some(FactPayload::Call {
                call_provenance, ..
            }) => Some(call_provenance),
            _ => None,
        }
    }

    pub(in crate::analysis) fn global_name(&self) -> Option<&SmolStr> {
        match self.provenance()? {
            SymbolCallProvenance::Global { name } => Some(name),
            _ => None,
        }
    }

    pub(in crate::analysis) fn matches_target(
        &self,
        target: &crate::api::rule::query::lifecycle::LifecycleCallTarget,
        names: &NameTable,
    ) -> bool {
        match target {
            crate::api::rule::query::lifecycle::LifecycleCallTarget::Global(name) => {
                self.global_name().is_some_and(|found| found == name)
            }
            crate::api::rule::query::lifecycle::LifecycleCallTarget::RootedMember(path) => self
                .chain()
                .and_then(|chain| names.lookup_path(path).map(|member| (member, chain)))
                .is_some_and(|(member, chain)| member == *chain && self.rooted()),
        }
    }

    pub(in crate::analysis) fn target(&self) -> Option<FunctionId> {
        match self.call_fact() {
            Some(FactPayload::Call {
                target_function, ..
            }) => *target_function,
            _ => None,
        }
    }

    pub(in crate::analysis) fn effective_args(&self) -> Option<&[CallArgInfo]> {
        match self.call_fact()? {
            FactPayload::Call { args, unwrap, .. } => Some(
                unwrap
                    .as_deref()
                    .map_or(args.as_slice(), |u| u.effective_args.as_slice()),
            ),
            _ => None,
        }
    }

    pub(in crate::analysis) fn matches_source(
        &self,
        flow: &crate::api::compiler::CompiledObjectFlow,
        names: &glass_lint_datastructures::NameTable,
    ) -> bool {
        let Some(args) = self.effective_args() else {
            return false;
        };
        let values = self.stream.values();
        flow.sources.iter().any(|source| {
            self.matches_target(&source.target, names)
                && source.arguments.iter().all(|matcher| {
                    args.get(matcher.index()).is_some_and(|argument| {
                        matcher.predicate().matches(argument, names, values)
                    })
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
}

#[derive(Clone, Debug)]
pub struct FunctionEffect {
    id: FunctionId,
    calls: Vec<EffectCall>,
    uses: Vec<EffectUse>,
    returns: Vec<ReturnProjection>,
    invalid: bool,
    value_roots: HashMap<ValueId, ValueId>,
    parameter_index: HashMap<ValueId, ParameterRef>,
}

impl FunctionEffect {
    fn record_property_write(
        &mut self,
        event: FactId,
        receiver: ValueId,
        property: Option<&str>,
        value_is_precise: bool,
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

    fn record_call(
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

    fn record_copy(&mut self, target: ValueId, source: ValueId) {
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

#[derive(Clone, Debug)]
pub struct FunctionEffects {
    by_id: FunctionTable<FunctionEffect>,
    budget_exhausted: bool,
    operation_count: usize,
}

impl Default for FunctionEffects {
    fn default() -> Self {
        Self {
            by_id: FunctionTable::new(0),
            budget_exhausted: false,
            operation_count: 0,
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

    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(in crate::analysis) fn operation_count(&self) -> usize {
        self.operation_count
    }

    pub(in crate::analysis) fn collect(stream: &FactStream<Frozen>, limit: usize) -> Self {
        let mut builder = FunctionEffectsBuilder::new(stream, limit);
        for fact in stream.facts() {
            builder.consume(fact, stream);
        }
        builder.finish()
    }
}

/// Mutable construction state for function effects.
///
/// Lowering can feed this builder while it projects the same frozen fact tape
/// into occurrence indexes. Keeping construction separate from the immutable
/// `FunctionEffects` value makes the shared derived pass explicit without
/// exposing either consumer's storage.
pub(in crate::analysis) struct FunctionEffectsBuilder {
    by_id: FunctionTable<FunctionEffect>,
    budget: Budget,
    value_provenance: HashMap<ValueId, SymbolCallProvenance>,
    enabled: bool,
}

impl FunctionEffectsBuilder {
    pub(in crate::analysis) fn new(stream: &FactStream<Frozen>, limit: usize) -> Self {
        let mut builder = Self {
            by_id: FunctionTable::new(stream.function_count()),
            budget: Budget::new(limit),
            value_provenance: HashMap::new(),
            enabled: stream.is_valid(),
        };
        if builder.enabled && builder.budget.try_push() {
            let _ = builder.by_id.insert(
                FunctionId::new(0),
                FunctionEffect {
                    id: FunctionId::new(0),
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots: HashMap::new(),
                    parameter_index: HashMap::new(),
                },
            );
        }
        builder
    }

    pub(in crate::analysis) fn consume(
        &mut self,
        fact: &SemanticFact,
        stream: &FactStream<Frozen>,
    ) {
        if !self.enabled {
            return;
        }
        if let FactPayload::Function {
            id,
            boundary: FunctionBoundary::Enter,
            ..
        } = &fact.payload
        {
            if !self.by_id.contains(*id) && !self.budget.try_push() {
                return;
            }
            let params = stream.function_parameters(*id);
            let _ = self.by_id.insert(
                *id,
                FunctionEffect {
                    id: *id,
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots: params.iter().map(|p| (p.value, p.value)).collect(),
                    parameter_index: params
                        .iter()
                        .map(|p| {
                            (
                                p.value,
                                ParameterRef {
                                    index: p.parameter_index,
                                    path: p.path,
                                },
                            )
                        })
                        .collect(),
                },
            );
            return;
        }

        let Some(effect) = self.by_id.get_mut(fact.function) else {
            return;
        };
        match &fact.payload {
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
                fact.id,
                *receiver,
                property.and_then(|id| stream.resolve_name(id)),
                *value_is_precise,
                stream,
                &mut self.budget,
            ),
            FactPayload::Call { .. } => effect.record_call(fact, stream, &mut self.budget),
            FactPayload::Control {
                kind: ControlKind::Return,
                return_value,
                ..
            } => effect.record_return(
                *return_value,
                &self.value_provenance,
                stream,
                &mut self.budget,
            ),
            _ => {}
        }
        effect.mark_unsupported_control(&fact.payload);
    }

    pub(in crate::analysis) fn finish(self) -> FunctionEffects {
        if !self.enabled {
            return FunctionEffects::default();
        }
        FunctionEffects {
            by_id: self.by_id,
            budget_exhausted: self.budget.exhausted(),
            operation_count: self.budget.used(),
        }
    }
}

#[cfg(test)]
mod tests;
