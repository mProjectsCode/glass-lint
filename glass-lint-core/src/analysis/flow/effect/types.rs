use std::borrow::Cow;

use glass_lint_datastructures::{NamePath, NameTable, PathId, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen},
    syntax::SymbolCallProvenance,
    value::{FunctionId, ValueId},
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct EffectCallId(pub(in crate::analysis) usize);

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectCall {
    pub(super) event: FactId,
    pub(super) arguments: Vec<EffectArgument>,
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
    pub(super) value: ValueId,
    pub(super) parameter: Option<ParameterRef>,
    pub(super) provenance: SymbolCallProvenance,
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
