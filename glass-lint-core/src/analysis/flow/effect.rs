//! Public-matcher-independent effects extracted from the canonical fact tape.
//!
//! Effects intentionally describe values and observable uses, never rules or
//! flow IDs.  The project linker supplies qualified call targets later; this
//! module only records the local relations needed by that linker.
//!
//! An effect becomes invalid when unsupported control flow or an effect budget
//! prevents a complete summary. Invalid summaries are not used for qualified
//! propagation, preserving fail-closed behavior across module boundaries.

use std::{borrow::Cow, collections::BTreeMap};

use glass_lint_datastructures::{Budget, NamePath, NameTable, PathId, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::{
    facts::{
        CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        ParameterBinding, SemanticFact,
    },
    flow::table::FunctionTable,
    syntax::SymbolCallProvenance,
    value::{FunctionId, ValueId},
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Dense index into a [`FunctionEffect`] call table.
pub(in crate::analysis) struct EffectCallId(pub(in crate::analysis) usize);

#[derive(Clone, Debug)]
/// Resolver-backed call relation retained for later project composition.
///
/// Only the fact identity and derived arguments (which include per-effect
/// parameter refs) are owned here.  Chain, result, provenance, rootedness, and
/// the qualified function target are borrowed from the canonical fact stream
/// through [`CallEffectRef`].
pub(in crate::analysis) struct EffectCall {
    /// Dense index of this call within the owning function's call table.
    #[allow(dead_code)]
    id: EffectCallId,
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
        /// Dense index into the owning function's call table.
        call_id: EffectCallId,
        /// Fact identity of the call, retained for cross-module event
        /// references that need the event without the owning effect.
        event: FactId,
        /// Zero-based argument position, resolved through the paired
        /// [`EffectCall`] held in the same [`FunctionEffect`].
        argument_index: usize,
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
    /// Lexical function identity owning this summary. Parameter bindings are
    /// looked up from the canonical fact stream via `parameters(stream)`.
    id: FunctionId,
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
    /// Value-id-to-parameter-ref index built once at construction, used by
    /// [`parameter_for`](Self::parameter_for) instead of a linear scan.
    parameter_index: BTreeMap<ValueId, ParameterRef>,
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
    #[allow(dead_code)]
    pub(in crate::analysis) fn id(&self) -> EffectCallId {
        self.id
    }

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

/// Borrowed fact view that provides chain, result, provenance, rootedness, and
/// the qualified function target for one call effect without copying them from
/// the canonical fact stream.
///
/// This is the single authority for effective-call selection (including
/// `.call()`/`.apply()` unwrapping) used by local flow, summaries, and
/// cross-module flow.
#[derive(Clone, Copy)]
pub(in crate::analysis) struct CallEffectRef<'stream> {
    pub(in crate::analysis) stream: &'stream FactStream<Frozen>,
    pub(in crate::analysis) event: FactId,
}

impl CallEffectRef<'_> {
    fn call_fact(&self) -> Option<&FactPayload> {
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

    /// Chain with a callee-name fallback that resolves one level of local
    /// binding.  Direct, rooted, and syntactic interned chains borrow from
    /// the fact stream; only the callee-name fallback returns an owned path.
    /// Used when the projector cannot rely on the resolver-backed chain
    /// because the call was summarized from facts.
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

    pub(in crate::analysis) fn target(&self) -> Option<FunctionId> {
        match self.call_fact() {
            Some(FactPayload::Call {
                target_function, ..
            }) => *target_function,
            _ => None,
        }
    }

    /// Return the effective call arguments, accounting for
    /// `.call()`/`.apply()` unwrapping.
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
        let Some(chain) = self.chain() else {
            return false;
        };
        flow.sources.iter().any(|source| {
            names
                .lookup_path(&source.member_call)
                .is_some_and(|member| member == *chain)
                && source.is_rooted == self.rooted()
                && source.arguments.iter().all(|matcher| {
                    args.get(matcher.index())
                        .is_some_and(|argument| matcher.matcher().matches(argument, names, values))
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

    /// Extract matcher-independent effects from the canonical fact stream in
    /// a single ordered pass: function-enter facts create effect slots before
    /// the events they own are processed.
    #[allow(clippy::too_many_lines)]
    pub(in crate::analysis) fn collect(stream: &FactStream<Frozen>, limit: usize) -> Self {
        let mut effects = Self::default();
        if !stream.is_valid() {
            return effects;
        }
        let mut budget = Budget::new(limit);
        let mut value_provenance = BTreeMap::new();

        // Pre-create the program-level function slot so top-level facts
        // have an effect to record into.
        if budget.try_push() {
            effects.by_id.insert(
                FunctionId(0),
                FunctionEffect {
                    id: FunctionId(0),
                    calls: Vec::new(),
                    uses: Vec::new(),
                    returns: Vec::new(),
                    invalid: false,
                    value_roots: BTreeMap::new(),
                    parameter_index: BTreeMap::new(),
                },
            );
        }

        for fact in stream.facts() {
            // Create function slot on first Function::Enter, which always
            // precedes the events owned by that function.
            if let FactPayload::Function {
                id,
                boundary: FunctionBoundary::Enter,
                ..
            } = &fact.payload
            {
                if !effects.by_id.contains(*id) && !budget.try_push() {
                    continue;
                }
                let params = stream.function_parameters(*id);
                effects.by_id.insert(
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
                // Function::Enter has no effect on the owning scope.
                continue;
            }

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
                    stream,
                    &mut budget,
                ),
                FactPayload::Call { .. } => effect.record_call(fact, stream, &mut budget),
                FactPayload::Control {
                    kind: ControlKind::Return,
                    return_value,
                    ..
                } => {
                    effect.record_return(*return_value, &value_provenance, stream, &mut budget);
                }
                _ => {}
            }
            effect.mark_unsupported_control(&fact.payload);
        }
        effects.budget_exhausted = budget.exhausted();
        effects.operation_count = budget.used();
        effects
    }
}

impl FunctionEffect {
    fn record_property_write(
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

    /// Parameter bindings captured at function entry, looked up from the
    /// canonical fact stream. Returns an empty slice for the program-level
    /// slot or when the stream is inaccessible.
    pub(in crate::analysis) fn parameters<'s>(
        &self,
        stream: &'s FactStream<Frozen>,
    ) -> &'s [ParameterBinding] {
        stream.function_parameters(self.id)
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

    /// Look up the argument at `index` inside the [`EffectCall`] identified
    /// by `call_id`.  Returns `None` when the call or index is not present.
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
                argument_index: argument.index,
            });
        }
        if budget.try_push() {
            self.calls.push(EffectCall {
                id: call_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{facts, resolution::Resolver};

    fn collect_effects(source: &str) -> (FactStream<Frozen>, FunctionEffects) {
        let parsed = crate::parse(source, "test.js").expect("source should parse");
        let mut resolver = Resolver::collect(&parsed.program, source);
        let stream = facts::build::build_test_stream(&parsed.program, &mut resolver);
        let effects = FunctionEffects::collect(&stream, usize::MAX);
        (stream, effects)
    }

    #[test]
    fn chain_owned_resolves_direct_call_with_rooted_or_syntactic_chain() {
        let (stream, _effects) = collect_effects("document.createElement('script');");
        let fact = stream
            .facts()
            .iter()
            .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .expect("call fact should exist");
        let cref = CallEffectRef {
            stream: &stream,
            event: fact.id,
        };
        let names = stream.names();
        let chain = cref
            .chain_owned(names)
            .expect("direct call should have a chain");
        let chain: &NamePath = &chain;
        assert!(
            names
                .resolve_path(chain)
                .is_some_and(|s| s.eq_chain("document.createElement")),
            "chain should be document.createElement, got {}",
            names
                .resolve_path(chain)
                .map_or_else(|| "(unresolvable)".to_string(), |s| s.to_string())
        );
        assert!(cref.chain().is_some(), "borrowed chain should exist");
        assert!(cref.rooted(), "global member call should be rooted");
    }

    #[test]
    fn chain_owned_falls_back_to_callee_name_for_alias_call() {
        let (stream, _effects) = collect_effects(
            "function fetch(url) { return url; } const alias = fetch; alias('/api');",
        );
        let names = stream.names();
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .collect();
        assert!(!call_facts.is_empty(), "expected at least 1 call fact");
        let alias_call = call_facts[0];
        let cref = CallEffectRef {
            stream: &stream,
            event: alias_call.id,
        };
        let chain = cref
            .chain_owned(names)
            .expect("alias call should have a chain via callee_name fallback");
        let chain: &NamePath = &chain;
        assert!(
            names
                .resolve_path(chain)
                .is_some_and(|s| s.eq_chain("alias")),
            "alias call chain should resolve to the callee name 'alias', got {:?}",
            names.resolve_path(chain)
        );
    }

    #[test]
    fn rooted_is_false_for_non_global_call() {
        let (stream, _effects) = collect_effects("function fn() { return 1; } fn();");
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .collect();
        assert!(!call_facts.is_empty(), "expected at least 1 call fact");
        let call_fact = call_facts[0];
        let cref = CallEffectRef {
            stream: &stream,
            event: call_fact.id,
        };
        assert!(!cref.rooted(), "local function call should not be rooted");
    }

    #[test]
    fn effective_args_unwraps_call_invocation() {
        let (stream, _effects) =
            collect_effects("function fetch(url) { return url; } fetch.call(null, '/api');");
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .collect();
        assert!(!call_facts.is_empty(), "expected at least 1 call fact");
        let call_fact = call_facts[0];
        let cref = CallEffectRef {
            stream: &stream,
            event: call_fact.id,
        };
        let effective = cref
            .effective_args()
            .expect(".call() should have effective args");
        assert_eq!(
            effective.len(),
            1,
            ".call() drops receiver, expected 1 arg, got {}",
            effective.len()
        );
        let values = stream.values();
        let is_api = effective[0].base_value != ValueId::UNKNOWN
            && values
                .static_string(effective[0].base_value)
                .is_some_and(|s| s == "/api");
        assert!(is_api, "effective arg should be '/api'");
    }

    #[test]
    fn effective_args_unwraps_apply_invocation() {
        let (stream, _effects) =
            collect_effects("function fetch(url) { return url; } fetch.apply(null, ['/api']);");
        let call_facts: Vec<_> = stream
            .facts()
            .iter()
            .filter(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .collect();
        assert!(!call_facts.is_empty(), "expected at least 1 call fact");
        let call_fact = call_facts[0];
        let cref = CallEffectRef {
            stream: &stream,
            event: call_fact.id,
        };
        let effective = cref
            .effective_args()
            .expect(".apply() should have effective args");
        assert_eq!(
            effective.len(),
            1,
            ".apply() drops receiver and unwraps, expected 1 arg, got {}",
            effective.len()
        );
        let values = stream.values();
        let is_api = effective[0].base_value != ValueId::UNKNOWN
            && values
                .static_string(effective[0].base_value)
                .is_some_and(|s| s == "/api");
        assert!(is_api, "effective arg should be '/api'");
    }

    #[test]
    fn call_fact_returns_none_for_unknown_id() {
        let (stream, _effects) = collect_effects("const x = 1;");
        let unknown = FactId(u32::MAX);
        let cref = CallEffectRef {
            stream: &stream,
            event: unknown,
        };
        assert!(cref.call_fact().is_none());
        assert!(cref.chain().is_none());
        assert!(!cref.rooted());
        assert_eq!(cref.result(), ValueId::UNKNOWN);
        assert!(cref.provenance().is_none());
        assert!(cref.target().is_none());
        assert!(cref.effective_args().is_none());
        let names = stream.names();
        assert!(cref.chain_owned(names).is_none());
    }

    #[test]
    fn chain_returns_borrowed_without_callee_name_fallback() {
        let (stream, _effects) = collect_effects("document.createElement('script');");
        let fact = stream
            .facts()
            .iter()
            .find(|f| matches!(&f.payload, FactPayload::Call { .. }))
            .expect("call fact should exist");
        let cref = CallEffectRef {
            stream: &stream,
            event: fact.id,
        };
        let names = stream.names();
        let owned = cref.chain_owned(names).unwrap();
        let borrowed = cref.chain().unwrap();
        assert_eq!(&*owned, borrowed, "owned chain should match borrowed");
    }

    #[test]
    fn call_argument_indexes_into_correct_call() {
        let (_stream, effects) = collect_effects(
            "function fn() { document.head.appendChild(document.createElement('script')); }",
        );
        let effect = effects
            .get(FunctionId(1))
            .expect("effect for fn should exist");
        let call = effect
            .calls()
            .iter()
            .find(|c| {
                c.arguments()
                    .iter()
                    .any(|a| a.index == 0 && a.value != ValueId::UNKNOWN)
            })
            .expect("appendChild call should exist");
        let call_id = call.id();
        let by_index = effect
            .call_argument(call_id, 0)
            .expect("argument at index 0 should exist");
        assert_eq!(by_index.index(), 0);
    }

    #[test]
    fn call_argument_returns_none_for_missing_index() {
        let (_stream, effects) =
            collect_effects("document.head.appendChild(document.createElement('script'));");
        let effect = effects
            .get(FunctionId(0))
            .expect("script effect should exist");
        let call = effect.calls().first().expect("call should exist");
        assert!(effect.call_argument(call.id(), 999).is_none());
        assert!(effect.call_argument(EffectCallId(usize::MAX), 0).is_none());
    }
}
