use glass_lint_datastructures::{NamePath, PathId, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::{
    facts::{CallArgInfo, FactId, FactPayload, FactStream, Frozen},
    model::{scope::FunctionId, value::ValueId},
    syntax::SymbolCallProvenance,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis) struct ParameterRef {
    pub(in crate::analysis::flow::effect) index: usize,
    pub(in crate::analysis::flow::effect) path: PathId,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectArgument {
    pub(in crate::analysis::flow::effect) index: usize,
    pub(in crate::analysis::flow::effect) value: ValueId,
    pub(in crate::analysis::flow::effect) path: PathId,
    pub(in crate::analysis::flow::effect) parameter: Option<ParameterRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct EffectCallId(usize);

impl EffectCallId {
    pub(in crate::analysis) fn new(index: usize) -> Self {
        Self(index)
    }

    pub(in crate::analysis) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct EffectCall {
    pub(in crate::analysis::flow::effect) event: FactId,
    pub(in crate::analysis::flow::effect) arguments: Vec<EffectArgument>,
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
        argument_index: usize,
    },
    CallReceiver {
        event: FactId,
        receiver: ParameterRef,
    },
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct ReturnProjection {
    pub(in crate::analysis::flow::effect) value: ValueId,
    pub(in crate::analysis::flow::effect) parameter: Option<ParameterRef>,
    pub(in crate::analysis::flow::effect) provenance: SymbolCallProvenance,
}

pub(in crate::analysis) struct CallShape<'a> {
    chain: Option<&'a NamePath>,
    rooted: bool,
    global_name: Option<&'a SmolStr>,
    arguments: &'a [CallArgInfo],
    result: ValueId,
    provenance: &'a SymbolCallProvenance,
    target: Option<FunctionId>,
    callee_chain: Option<NamePath>,
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
}

impl FactStream<Frozen> {
    /// Resolve a call fact into its matching shape, failing closed for unknown
    /// IDs and non-call facts.
    pub(in crate::analysis) fn call_shape(&self, event: FactId) -> Option<CallShape<'_>> {
        let fact = self.fact(event)?;
        let FactPayload::Call(call) = fact.payload() else {
            return None;
        };
        let chain = call
            .unwrap()
            .and_then(|call| call.chain_path.as_ref())
            .or_else(|| call.rooted_chain())
            .or_else(|| call.syntactic_path());
        let global_name = match call.call_provenance() {
            SymbolCallProvenance::Global { name } => Some(name),
            _ => None,
        };
        let callee_chain = if chain.is_none() {
            call.callee_name()
                .and_then(|id| self.resolve_name(id))
                .and_then(|name| self.names().lookup_path(&SymbolPath::from(name)))
        } else {
            None
        };
        Some(CallShape {
            chain,
            rooted: call.rooted_chain().is_some(),
            global_name,
            arguments: call.effective_args(),
            result: call.result(),
            provenance: call.call_provenance(),
            target: call.target_function(),
            callee_chain,
        })
    }
}

impl CallShape<'_> {
    /// Member chain for requirement matching and rooted-member candidacy,
    /// resolved in one place: wrapper chain, then rooted chain, then
    /// syntactic path, then the callee-name fallback. Fail-closed: an
    /// unresolvable call yields no chain rather than an invented path.
    pub(in crate::analysis) fn chain(&self) -> Option<&NamePath> {
        self.chain.or(self.callee_chain.as_ref())
    }

    pub(in crate::analysis) fn rooted(&self) -> bool {
        self.rooted
    }

    pub(in crate::analysis) fn result(&self) -> ValueId {
        self.result
    }

    pub(in crate::analysis) fn provenance(&self) -> &SymbolCallProvenance {
        self.provenance
    }

    pub(in crate::analysis) fn global_name(&self) -> Option<&SmolStr> {
        self.global_name
    }

    pub(in crate::analysis) fn target(&self) -> Option<FunctionId> {
        self.target
    }

    pub(in crate::analysis) fn effective_args(&self) -> &[CallArgInfo] {
        self.arguments
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
