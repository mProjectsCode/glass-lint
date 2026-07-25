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
use std::collections::BTreeMap;

use glass_lint_datastructures::{Budget, NamePath, NameTable, PathId, SymbolPath};
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

#[derive(Clone, Debug)]
pub struct FunctionEffect {
    id: FunctionId,
    calls: Vec<EffectCall>,
    uses: Vec<EffectUse>,
    returns: Vec<ReturnProjection>,
    invalid: bool,
    value_roots: BTreeMap<ValueId, ValueId>,
    parameter_index: BTreeMap<ValueId, ParameterRef>,
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

#[derive(Clone, Debug, Default)]
pub struct FunctionEffects {
    by_id: FunctionTable<FunctionEffect>,
    budget_exhausted: bool,
    operation_count: usize,
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

    #[allow(clippy::too_many_lines)]
    pub(in crate::analysis) fn collect(stream: &FactStream<Frozen>, limit: usize) -> Self {
        let mut effects = Self::default();
        if !stream.is_valid() {
            return effects;
        }
        let mut budget = Budget::new(limit);
        let mut value_provenance = BTreeMap::new();

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

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::NamePath;

    use super::*;
    use crate::analysis::{
        facts,
        facts::{FactId, FactPayload, FactStream, Frozen},
        resolution::Resolver,
        value::{FunctionId, ValueId},
    };

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
        let by_index = effect
            .call_argument(EffectCallId(0), 0)
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
        assert!(effect.call_argument(EffectCallId(0), 999).is_none());
        assert!(effect.call_argument(EffectCallId(usize::MAX), 0).is_none());
    }
}
