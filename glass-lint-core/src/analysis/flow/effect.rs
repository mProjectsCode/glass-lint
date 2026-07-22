//! Public-matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.
//!
//! An effect becomes invalid when unsupported control flow or an effect budget
//! prevents a complete summary. Invalid summaries are not used for qualified
//! propagation, preserving fail-closed behavior across module boundaries.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::{
    analysis::{
        facts::{
            CallArgInfo, ControlKind, FactId, FactPayload, FactStream,
            ParameterBinding, SemanticFact,
        },
        flow::table::FunctionTable,
        syntax::SymbolCallProvenance,
        value::{FunctionId, NamePath, PathId, ValueId},
    },
    budget::Budget,
};

#[derive(Clone, Debug, Eq, PartialEq)]
/// A parameter identity plus the destructured path that selects it.
pub(in crate::analysis) struct ParameterRef {
    /// Zero-based top-level parameter index.
    index: usize,
    /// Path within that parameter's argument value.
    path: PathId,
}

#[derive(Clone, Debug)]
/// One call argument as represented inside a function effect.
pub(in crate::analysis) struct EffectArgument {
    /// Zero-based argument position at the call site.
    index: usize,
    /// Value identity observed at that position.
    value: ValueId,
    /// Static path from the argument root.
    path: PathId,
    /// Parameter identity when this argument aliases the current function.
    parameter: Option<ParameterRef>,
}

#[derive(Clone, Debug)]
/// Resolver-backed call relation retained for later project composition.
///
/// Only the fact identity and derived arguments (which include per-effect
/// parameter refs) are owned here.  Chain, result, provenance, rootedness, and
/// the qualified function target are borrowed from the canonical fact stream
/// through [`CallEffectRef`].
pub(in crate::analysis) struct EffectCall {
    /// Fact identity of the call event.
    event: FactId,
    /// Arguments projected to parameter paths.
    arguments: Vec<EffectArgument>,
}

#[derive(Clone, Debug)]
/// Observable uses of a parameter or source-root value.
pub(in crate::analysis) enum EffectUse {
    PropertyWrite {
        /// Fact identity of the write.
        event: FactId,
        /// Written receiver when it aliases a parameter.
        receiver: Option<ParameterRef>,
        /// Receiver identity observed at the write, used for source-root
        /// matching when no parameter context is active.
        receiver_value: ValueId,
        /// Static property name, if proven.
        property: Option<SmolStr>,
    },
    CallArgument {
        /// Fact identity of the call.
        event: FactId,
        /// Argument identity passed to the call.
        argument: EffectArgument,
    },
    CallReceiver {
        /// Fact identity of the member call.
        event: FactId,
        /// Receiver parameter consumed by the member call.
        receiver: ParameterRef,
    },
}

#[derive(Clone, Debug)]
pub struct FunctionEffect {
    /// This summary is rule-independent; matcher policy is applied only when
    /// it is projected into local or qualified flow states.
    /// Lexical function identity owning this summary.
    id: FunctionId,
    /// Parameter bindings at function entry.
    parameters: Vec<ParameterBinding>,
    /// Calls made by this function in source order.
    calls: Vec<EffectCall>,
    /// Observable parameter/source uses in source order.
    uses: Vec<EffectUse>,
    /// Values returned by this function.
    returns: Vec<ReturnProjection>,
    /// True when this summary cannot safely describe the full function.
    invalid: bool,
    /// Source-order value copies. Project flow uses this to connect a source
    /// call result through local declarations before a qualified call.
    value_roots: BTreeMap<ValueId, ValueId>,
}

#[derive(Clone, Debug)]
/// One return value and its optional parameter provenance.
pub(in crate::analysis) struct ReturnProjection {
    /// Returned value identity.
    value: ValueId,
    /// Parameter path if the return forwards an input value.
    parameter: Option<ParameterRef>,
    /// Provenance retained for source matching.
    provenance: SymbolCallProvenance,
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
        stream: &'s FactStream,
    ) -> CallEffectRef<'s> {
        CallEffectRef {
            stream,
            event: self.event,
        }
    }
}

/// Borrowed fact view that provides chain, result, provenance, rootedness, and
/// the qualified function target for one call effect without copying them from
/// the canonical fact stream.
///
/// This is the single authority for effective-call selection (including
/// `.call()`/`.apply()` unwrapping) used by local flow, summaries, and
/// cross-module flow.
#[derive(Clone, Copy)]
pub(in crate::analysis) struct CallEffectRef<'stream> {
    pub(in crate::analysis) stream: &'stream FactStream,
    pub(in crate::analysis) event: FactId,
}

impl CallEffectRef<'_> {
    fn call_fact(&self) -> &FactPayload {
        &self
            .stream
            .fact(self.event)
            .expect("call fact must exist")
            .payload
    }

    pub(in crate::analysis) fn chain(&self) -> Option<&NamePath> {
        match self.call_fact() {
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

    pub(in crate::analysis) fn rooted(&self) -> bool {
        matches!(self.call_fact(), FactPayload::Call {
            rooted_chain: Some(_),
            ..
        })
    }

    pub(in crate::analysis) fn result(&self) -> ValueId {
        match self.call_fact() {
            FactPayload::Call { result, .. } => *result,
            _ => ValueId::UNKNOWN,
        }
    }

    pub(in crate::analysis) fn provenance(&self) -> &SymbolCallProvenance {
        match self.call_fact() {
            FactPayload::Call {
                call_provenance, ..
            } => call_provenance,
            _ => unreachable!("must be a Call fact"),
        }
    }

    pub(in crate::analysis) fn target(&self) -> Option<FunctionId> {
        match self.call_fact() {
            FactPayload::Call {
                target_function, ..
            } => *target_function,
            _ => None,
        }
    }

    pub(in crate::analysis) fn matches_source(
        &self,
        flow: &crate::api::compiler::CompiledObjectFlow,
        names: &crate::analysis::name::NameTable,
    ) -> bool {
        let Some(args) = self.stream.call_args_for_event(self.event) else {
            return false;
        };
        let values = self.stream.values();
        let Some(chain) = self.chain() else {
            return false;
        };
        flow.sources.iter().any(|source| {
            NamePath::from_symbol_path(&source.member_call, names)
                .is_some_and(|member| member == *chain)
                && source.provenance.matches_rooted(self.rooted())
                && source.arguments.iter().all(|matcher| {
                    args.get(matcher.index()).is_some_and(|argument| {
                        values.is_some_and(|values| {
                            matcher.matcher().matches(argument, names, values)
                        })
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

#[derive(Clone, Debug, Default)]
/// All local function effects for one module, indexed by function identity.
pub struct FunctionEffects {
    /// Dense sparse table of summaries keyed by function ID.
    by_id: FunctionTable<FunctionEffect>,
    /// Local effect limits fail closed and are surfaced as a project
    /// diagnostic instead of looking like a clean analysis.
    budget_exhausted: bool,
    /// Successfully retained effect records, including function summaries.
    operation_count: usize,
}

impl FunctionEffects {
    /// Look up one function summary without treating a missing ID as valid.
    pub(in crate::analysis) fn get(&self, id: FunctionId) -> Option<&FunctionEffect> {
        self.by_id.get(id)
    }

    /// Iterate summaries in deterministic function-ID order.
    pub(in crate::analysis) fn iter_effects(&self) -> impl Iterator<Item = &FunctionEffect> {
        self.by_id.values()
    }

    /// Report whether effect limits prevented a complete local summary.
    pub(in crate::analysis) fn budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(in crate::analysis) fn operation_count(&self) -> usize {
        self.operation_count
    }

    /// Extract matcher-independent effects from the canonical fact stream.
    pub(in crate::analysis) fn collect(stream: &FactStream, limit: usize) -> Self {
        let mut effects = Self::default();
        if !stream.is_valid() {
            return effects;
        }
        if stream.names().is_none() {
            return effects;
        }
        let mut budget = Budget::new(limit);
        let mut value_provenance = BTreeMap::new();
        effects.initialize(stream, &mut budget);

        for fact in stream.facts() {
            let Some(effect) = effects.by_id.get_mut(fact.function) else {
                continue;
            };
            match &fact.payload {
                FactPayload::Reference { value, provenance } => {
                    effect.record_reference(*value, provenance, &mut value_provenance);
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
                } => effect.record_property_write(
                    fact.id,
                    *receiver,
                    property.and_then(|id| stream.resolve_name(id)),
                    &mut budget,
                ),
                FactPayload::Call { .. } => effect.record_call(fact, &mut budget),
                FactPayload::Control {
                    kind: ControlKind::Return,
                    return_value,
                    ..
                } => effect.record_return(*return_value, &value_provenance, &mut budget),
                _ => {}
            }
            effect.mark_unsupported_control(&fact.payload);
        }
        effects.budget_exhausted = budget.exhausted();
        effects.operation_count = budget.used();
        effects
    }

    fn initialize(&mut self, stream: &FactStream, budget: &mut Budget) {
        if stream.names().is_none() {
            return;
        }
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
            if !self.by_id.contains(*id) && !budget.try_push() {
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
        if budget.try_push() {
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
}

impl FunctionEffect {
    fn record_property_write(
        &mut self,
        event: FactId,
        receiver: ValueId,
        property: Option<&str>,
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
        });
    }
}

impl FunctionEffect {
    /// Function identity owning this summary.
    pub(in crate::analysis) fn id(&self) -> FunctionId {
        self.id
    }

    /// Calls in canonical fact order.
    pub(in crate::analysis) fn calls(&self) -> &[EffectCall] {
        &self.calls
    }

    /// Observable uses in canonical fact order.
    pub(in crate::analysis) fn uses(&self) -> &[EffectUse] {
        &self.uses
    }

    /// Parameter bindings captured at function entry.
    pub(in crate::analysis) fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }

    /// Return projections captured by the summary.
    pub(in crate::analysis) fn returns(&self) -> &[ReturnProjection] {
        &self.returns
    }

    /// Whether the summary must be rejected by project flow.
    pub(in crate::analysis) fn is_invalid(&self) -> bool {
        self.invalid
    }

    /// Resolve a value to its known parameter/source root.
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

    fn record_call(&mut self, fact: &SemanticFact, budget: &mut Budget) {
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
        let arguments = self.build_effect_arguments(effective_args);
        if budget.try_push() {
            self.calls.push(EffectCall {
                event: fact.id,
                arguments: arguments.clone(),
            });
        } else {
            self.invalid = true;
        }
        if let Some(receiver) = receiver.and_then(|value| self.parameter_for(value)) {
            if budget.try_push() {
                self.uses.push(EffectUse::CallReceiver {
                    event: fact.id,
                    receiver,
                });
            } else {
                self.invalid = true;
            }
        }
        for argument in arguments {
            if !budget.try_push() {
                self.invalid = true;
                break;
            }
            self.uses.push(EffectUse::CallArgument {
                event: fact.id,
                argument,
            });
        }
        self.value_roots.entry(*result).or_insert(*result);
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
        provenance: &SymbolCallProvenance,
        value_provenance: &mut BTreeMap<ValueId, SymbolCallProvenance>,
    ) {
        value_provenance.insert(value, provenance.clone());
        if value != ValueId::UNKNOWN {
            self.value_roots.entry(value).or_insert(value);
        }
    }

    fn record_return(
        &mut self,
        value: ValueId,
        value_provenance: &BTreeMap<ValueId, SymbolCallProvenance>,
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
