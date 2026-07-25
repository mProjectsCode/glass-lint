#![allow(private_interfaces)]

use glass_lint_datastructures::{ByteRange, NameId, NamePath, PathId};
use smol_str::SmolStr;

use crate::analysis::{
    model::{scope::FunctionId, value::ValueId},
    syntax::{SymbolCallProvenance, SymbolMemberProvenance},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactId(pub u32);

impl FactId {
    #[cfg(test)]
    pub fn from_index(index: usize) -> Option<Self> {
        if index < MAX_FACTS {
            Some(Self(u32::try_from(index).ok()?))
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
pub struct ControlRegionId(pub u32);

#[cfg(test)]
mod control_region_tests {
    use super::*;

    #[test]
    fn control_regions_are_typed_and_orderable() {
        assert!(ControlRegionId(1) < ControlRegionId(2));
        assert_eq!(ControlRegionId::default(), ControlRegionId(0));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactKind {
    Declaration,
    Assignment,
    PropertyWrite,
    Call,
    Construction,
    Reference,
    MemberRead,
    Function,
    Control,
}

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
    InstanceofOperand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionBoundary {
    Enter,
    Exit,
}

#[derive(Debug, Clone)]
pub struct CallArgInfo {
    pub value: ValueId,
    pub base_value: ValueId,
    pub base_path: PathId,
    pub spread: bool,
    pub provenance: SymbolCallProvenance,
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

pub struct ArgumentView<'a> {
    pub argument: &'a CallArgInfo,
    pub static_string: Option<&'a str>,
}

impl<'a> ArgumentView<'a> {
    pub fn new(argument: &'a CallArgInfo) -> Self {
        Self {
            argument,
            static_string: None,
        }
    }

    pub fn with_static_string(mut self, value: &'a str) -> Self {
        self.static_string = Some(value);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterBinding {
    pub parameter_index: usize,
    pub path: PathId,
    pub value: ValueId,
    pub default: Option<ValueId>,
    pub rest: bool,
}

#[derive(Debug, Clone)]
pub struct CallUnwrap {
    pub chain_path: Option<NamePath>,
    pub effective_args: Vec<CallArgInfo>,
}

#[derive(Debug, Clone)]
pub enum FactPayload {
    Reference {
        value: ValueId,
        provenance: SymbolCallProvenance,
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
        value: ValueId,
    },
    Call {
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
    },
    Function {
        id: FunctionId,
        boundary: FunctionBoundary,
    },
    Control {
        kind: ControlKind,
        region: ControlRegionId,
        return_value: ValueId,
    },
    Construction {
        callee_span: ByteRange,
        callee_name: Option<NameId>,
        provenance: SymbolCallProvenance,
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
pub struct SemanticFact {
    pub id: FactId,
    pub span: ByteRange,
    pub function: FunctionId,
    #[cfg(test)]
    pub kind: FactKind,
    pub payload: FactPayload,
}

impl SemanticFact {
    pub fn new(
        id: FactId,
        span: ByteRange,
        function: FunctionId,
        kind: FactKind,
        payload: FactPayload,
    ) -> Self {
        #[cfg(not(test))]
        let _ = kind;
        Self {
            id,
            span,
            function,
            #[cfg(test)]
            kind,
            payload,
        }
    }

    #[cfg(test)]
    pub fn id(&self) -> FactId {
        self.id
    }

    #[cfg(test)]
    pub fn kind(&self) -> FactKind {
        self.kind
    }
}

pub const MAX_FACTS: usize = 1 << 20;
