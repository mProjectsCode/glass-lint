use glass_lint_datastructures::{ByteRange, NameId, NamePath, PathId};
use smol_str::SmolStr;

use crate::analysis::{
    facts::stream::FactStreamToken,
    model::{
        scope::FunctionId,
        value::{StaticObject, ValueId},
    },
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactId(u32);

impl FactId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(in crate::analysis) const fn raw(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn raw_for_test(self) -> u32 {
        self.raw()
    }

    #[cfg(test)]
    pub fn from_index(index: usize) -> Option<Self> {
        if index < MAX_FACTS {
            Some(Self::new(u32::try_from(index).ok()?))
        } else {
            None
        }
    }

    pub fn index(self) -> Option<usize> {
        let idx = self.0 as usize;
        if idx < MAX_FACTS { Some(idx) } else { None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ControlRegionId(u32);

impl ControlRegionId {
    pub(in crate::analysis) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub(in crate::analysis) const fn raw(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub(in crate::analysis) const fn from_test(raw: u32) -> Self {
        Self::new(raw)
    }
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    BranchStart,
    BranchThen,
    BranchElse,
    BranchEnd,
    LoopStart { guaranteed: bool },
    LoopUpdate,
    LoopEnd,
    SwitchStart,
    SwitchCase { is_default: bool },
    SwitchEnd,
    TryStart,
    CatchStart,
    FinallyStart,
    TryEnd,
    Break,
    Continue,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassFactRole {
    Declaration,
    SuperclassOperand,
    InstanceofOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionBoundary {
    Enter,
    Exit,
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallArgInfo {
    pub(in crate::analysis) value: ValueId,
    pub(in crate::analysis) base_value: ValueId,
    pub(in crate::analysis) base_path: PathId,
    pub(in crate::analysis) spread: bool,
    pub(in crate::analysis) provenance: SymbolCallProvenance,
}

impl CallArgInfo {
    pub fn unknown() -> Self {
        Self {
            value: ValueId::UNKNOWN,
            base_value: ValueId::UNKNOWN,
            base_path: PathId::EMPTY,
            spread: false,
            provenance: crate::analysis::syntax::SymbolCallProvenance::Local,
        }
    }
}

pub(in crate::analysis) struct ArgumentView<'a> {
    pub(in crate::analysis) argument: &'a CallArgInfo,
    pub(in crate::analysis) static_string: Option<&'a str>,
    pub(in crate::analysis) object: Option<&'a StaticObject>,
    pub(in crate::analysis) rooted_chain: Option<&'a NamePath>,
}

impl<'a> ArgumentView<'a> {
    pub fn new(argument: &'a CallArgInfo) -> Self {
        Self {
            argument,
            static_string: None,
            object: None,
            rooted_chain: None,
        }
    }

    pub fn with_static_string(mut self, value: &'a str) -> Self {
        self.static_string = Some(value);
        self
    }

    pub fn with_static_object(mut self, object: Option<&'a StaticObject>) -> Self {
        self.object = object;
        self
    }

    pub fn with_rooted_chain(mut self, chain: Option<&'a NamePath>) -> Self {
        self.rooted_chain = chain;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::analysis) struct ParameterBinding {
    parameter_index: usize,
    path: PathId,
    value: ValueId,
    default: Option<ValueId>,
    rest: bool,
}

impl ParameterBinding {
    pub(in crate::analysis) fn new(
        parameter_index: usize,
        path: PathId,
        value: ValueId,
        default: Option<ValueId>,
        rest: bool,
    ) -> Self {
        Self {
            parameter_index,
            path,
            value,
            default,
            rest,
        }
    }

    pub(in crate::analysis) fn parameter_index(&self) -> usize {
        self.parameter_index
    }

    pub(in crate::analysis) fn path(&self) -> PathId {
        self.path
    }

    pub(in crate::analysis) fn value(&self) -> ValueId {
        self.value
    }

    pub(in crate::analysis) fn default_value(&self) -> Option<ValueId> {
        self.default
    }

    pub(in crate::analysis) fn is_rest(&self) -> bool {
        self.rest
    }

    pub(in crate::analysis) fn is_root_for(&self, argument_index: usize) -> bool {
        self.parameter_index == argument_index && self.path.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallUnwrap {
    pub(in crate::analysis) chain_path: Option<NamePath>,
    pub(in crate::analysis) effective_args: Vec<CallArgInfo>,
}

/// Fact-model owner for the semantic call-event contract. Producers use the
/// named constructors and consumers use semantic accessors instead of the
/// storage shape.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct CallEvent {
    callee: ValueId,
    receiver: Option<ValueId>,
    result: ValueId,
    callee_span: ByteRange,
    callee_name: Option<NameId>,
    call_provenance: SymbolCallProvenance,
    syntactic_path: Option<NamePath>,
    rooted_chain: Option<NamePath>,
    module_member: Option<SymbolMemberProvenance>,
    returned_member: Option<(NamePath, NamePath)>,
    instance_class: Option<(SmolStr, SmolStr)>,
    target_function: Option<FunctionId>,
    args: Vec<CallArgInfo>,
    unwrap: Option<Box<CallUnwrap>>,
}

impl CallEvent {
    pub(in crate::analysis) fn unknown(
        result: ValueId,
        callee_span: ByteRange,
        call_provenance: SymbolCallProvenance,
        args: Vec<CallArgInfo>,
    ) -> Self {
        Self {
            callee: ValueId::UNKNOWN,
            receiver: None,
            result,
            callee_span,
            callee_name: None,
            call_provenance,
            syntactic_path: None,
            rooted_chain: None,
            module_member: None,
            returned_member: None,
            instance_class: None,
            target_function: None,
            args,
            unwrap: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::analysis) fn resolved(
        callee: ValueId,
        receiver: Option<ValueId>,
        result: ValueId,
        callee_span: ByteRange,
        callee_name: Option<NameId>,
        call_provenance: SymbolCallProvenance,
        syntactic_path: Option<NamePath>,
        rooted_chain: Option<NamePath>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(NamePath, NamePath)>,
        instance_class: Option<(SmolStr, SmolStr)>,
        target_function: Option<FunctionId>,
        args: Vec<CallArgInfo>,
        unwrap: Option<Box<CallUnwrap>>,
    ) -> Self {
        Self {
            callee,
            receiver,
            result,
            callee_span,
            callee_name,
            call_provenance,
            syntactic_path,
            rooted_chain,
            module_member,
            returned_member,
            instance_class,
            target_function,
            args,
            unwrap,
        }
    }

    pub(in crate::analysis) fn callee(&self) -> ValueId {
        self.callee
    }

    pub(in crate::analysis) fn receiver(&self) -> Option<ValueId> {
        self.receiver
    }

    pub(in crate::analysis) fn result(&self) -> ValueId {
        self.result
    }

    pub(in crate::analysis) fn callee_span(&self) -> ByteRange {
        self.callee_span
    }

    pub(in crate::analysis) fn callee_name(&self) -> Option<NameId> {
        self.callee_name
    }

    pub(in crate::analysis) fn call_provenance(&self) -> &SymbolCallProvenance {
        &self.call_provenance
    }

    pub(in crate::analysis) fn syntactic_path(&self) -> Option<&NamePath> {
        self.syntactic_path.as_ref()
    }

    pub(in crate::analysis) fn rooted_chain(&self) -> Option<&NamePath> {
        self.rooted_chain.as_ref()
    }

    pub(in crate::analysis) fn module_member(&self) -> Option<&SymbolMemberProvenance> {
        self.module_member.as_ref()
    }

    pub(in crate::analysis) fn returned_member(&self) -> Option<&(NamePath, NamePath)> {
        self.returned_member.as_ref()
    }

    pub(in crate::analysis) fn instance_class(&self) -> Option<&(SmolStr, SmolStr)> {
        self.instance_class.as_ref()
    }

    pub(in crate::analysis) fn target_function(&self) -> Option<FunctionId> {
        self.target_function
    }

    pub(in crate::analysis) fn args(&self) -> &[CallArgInfo] {
        &self.args
    }

    pub(in crate::analysis) fn unwrap(&self) -> Option<&CallUnwrap> {
        self.unwrap.as_deref()
    }

    pub(in crate::analysis) fn effective_args(&self) -> &[CallArgInfo] {
        self.unwrap()
            .map_or_else(|| self.args(), |call| call.effective_args.as_slice())
    }
}

impl FactPayload {
    /// Return the arguments visible to call-effect and constraint consumers.
    /// Wrapper calls replace authored arguments with their bound/effective
    /// projection; ordinary calls retain their authored argument list.
    pub(in crate::analysis) fn effective_call_args(&self) -> Option<&[CallArgInfo]> {
        let Self::Call(call) = self else { return None };
        Some(call.effective_args())
    }
}

#[derive(Debug, Clone)]
pub(in crate::analysis) enum FactPayload {
    Reference {
        value: ValueId,
        provenance: SymbolCallProvenance,
        /// Source span of the defining literal for a direct static-string
        /// alias, when it is available in this file.
        static_string_origin: Option<ByteRange>,
    },
    MemberRead {
        syntactic_path: Option<NamePath>,
        rooted_chain: Option<NamePath>,
        module_member: Option<SymbolMemberProvenance>,
        returned_member: Option<(NamePath, NamePath)>,
    },
    Declaration {
        target: ValueId,
        source: ValueId,
    },
    Assignment {
        target: ValueId,
        source: ValueId,
        receiver: Option<ValueId>,
    },
    PropertyWrite {
        receiver: ValueId,
        property: Option<NameId>,
        rooted_chain: Option<NamePath>,
        value: ValueId,
        value_is_precise: bool,
    },
    Call(CallEvent),
    Function {
        id: FunctionId,
        boundary: FunctionBoundary,
    },
    Control {
        kind: ControlKind,
        region: ControlRegionId,
    },
    Return {
        region: ControlRegionId,
        value: ValueId,
    },
    Construction {
        callee_span: ByteRange,
        callee_name: Option<NameId>,
        provenance: SymbolCallProvenance,
        rooted_chain: Option<NamePath>,
    },
    Import {
        module: String,
    },
    Class {
        name: Option<SmolStr>,
        role: ClassFactRole,
        provenance: Option<(SmolStr, SmolStr)>,
    },
}

#[derive(Debug, Clone)]
pub(in crate::analysis) struct SemanticFact {
    pub(in crate::analysis) id: FactId,
    pub(in crate::analysis) span: ByteRange,
    pub(in crate::analysis) function: FunctionId,
    pub(in crate::analysis) payload: FactPayload,
}

impl SemanticFact {
    pub(in crate::analysis) fn new(
        _authority: FactStreamToken,
        id: FactId,
        span: ByteRange,
        function: FunctionId,
        payload: FactPayload,
    ) -> Self {
        Self {
            id,
            span,
            function,
            payload,
        }
    }

    #[cfg(test)]
    pub fn id(&self) -> FactId {
        self.id
    }
}

pub const MAX_FACTS: usize = 1 << 20;

/// Marker type for the mutable building phase of [`FactStream`].
#[derive(Debug)]
pub(in crate::analysis) struct Building;

/// Marker type for the immutable frozen phase of [`FactStream`].
/// Accessors for names and values are only available in this phase.
#[derive(Debug)]
pub(in crate::analysis) struct Frozen;
